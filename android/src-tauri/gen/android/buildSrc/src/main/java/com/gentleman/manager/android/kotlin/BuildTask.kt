import java.io.File
import org.apache.tools.ant.taskdefs.condition.Os
import org.gradle.api.DefaultTask
import org.gradle.api.GradleException
import org.gradle.api.logging.LogLevel
import org.gradle.api.tasks.Input
import org.gradle.api.tasks.TaskAction

open class BuildTask : DefaultTask() {
    @Input
    var rootDirRel: String? = null

    @Input
    var target: String? = null

    @Input
    var release: Boolean? = null

    @TaskAction
    fun assemble() {
        val executable = "pnpm"
        try {
            runTauriCli(executable)
            return
        } catch (e: Exception) {
            if (!Os.isFamily(Os.FAMILY_WINDOWS)) {
                throw e
            }

            val fallbacks = listOf("$executable.exe", "$executable.cmd", "$executable.bat")
            var lastException: Exception = e
            for (fallback in fallbacks) {
                try {
                    runTauriCli(fallback)
                    return
                } catch (fallbackException: Exception) {
                    lastException = fallbackException
                }
            }

            if (shouldFallbackToCargo(lastException)) {
                logger.lifecycle("Tauri Android CLI failed on Windows; fallback to cargo build + copy jniLibs")
                runCargoBuildAndCopy()
                return
            }

            throw lastException
        }
    }

    private fun shouldFallbackToCargo(e: Exception): Boolean {
        val message = e.toString()
        return message.contains("symbolic link", ignoreCase = true) ||
            message.contains("pnpm.bat", ignoreCase = true) ||
            message.contains("android-studio-script", ignoreCase = true)
    }

    private fun runCargoBuildAndCopy() {
        val rootDirRel = rootDirRel ?: throw GradleException("rootDirRel cannot be null")
        val shortTarget = target ?: throw GradleException("target cannot be null")
        val isRelease = release ?: throw GradleException("release cannot be null")

        val targetTriple = when (shortTarget) {
            "aarch64", "arm64" -> "aarch64-linux-android"
            "armv7", "arm" -> "armv7-linux-androideabi"
            "i686", "x86" -> "i686-linux-android"
            "x86_64" -> "x86_64-linux-android"
            else -> throw GradleException("Unsupported target: $shortTarget")
        }

        val abi = when (shortTarget) {
            "aarch64", "arm64" -> "arm64-v8a"
            "armv7", "arm" -> "armeabi-v7a"
            "i686", "x86" -> "x86"
            "x86_64" -> "x86_64"
            else -> throw GradleException("Unsupported ABI target: $shortTarget")
        }

        val androidHome = System.getenv("ANDROID_HOME")
            ?: throw GradleException("ANDROID_HOME is not set")

        val ndkHome = System.getenv("NDK_HOME")
            ?: System.getenv("ANDROID_NDK_HOME")
            ?: File(androidHome, "ndk/26.1.10909125").absolutePath

        val toolchainBin = File(ndkHome, "toolchains/llvm/prebuilt/windows-x86_64/bin")
        if (!toolchainBin.exists()) {
            throw GradleException("NDK toolchain not found: ${toolchainBin.absolutePath}")
        }

        val clangCmd = when (targetTriple) {
            "aarch64-linux-android" -> "aarch64-linux-android21-clang.cmd"
            "armv7-linux-androideabi" -> "armv7a-linux-androideabi21-clang.cmd"
            "i686-linux-android" -> "i686-linux-android21-clang.cmd"
            "x86_64-linux-android" -> "x86_64-linux-android21-clang.cmd"
            else -> throw GradleException("Unsupported target triple: $targetTriple")
        }

        val clangPath = File(toolchainBin, clangCmd).absolutePath
        val arPath = File(toolchainBin, "llvm-ar.exe").absolutePath

        val root = File(project.projectDir, rootDirRel)
        val manifestPath = File(root, "Cargo.toml")

        val targetEnvKey = targetTriple.uppercase().replace('-', '_')
        val ccEnvSuffix = targetTriple.replace('-', '_')

        project.exec {
            workingDir(root)
            executable("cargo")
            args("build")
            args("--manifest-path", manifestPath.absolutePath)
            args("--target", targetTriple)
            args("--features", "tauri/custom-protocol tauri/custom-protocol")
            args("--lib")
            if (isRelease) {
                args("--release")
            }

            environment("ANDROID_HOME", androidHome)
            environment("NDK_HOME", ndkHome)
            environment("ANDROID_NDK_HOME", ndkHome)
            environment("CC_$ccEnvSuffix", clangPath)
            environment("AR_$ccEnvSuffix", arPath)
            environment("CARGO_TARGET_${targetEnvKey}_LINKER", clangPath)
            environment("PATH", "${toolchainBin.absolutePath}${File.pathSeparator}${System.getenv("PATH")}")
        }.assertNormalExitValue()

        val profileDir = if (isRelease) "release" else "debug"
        val builtLibDir = File(root, "target/$targetTriple/$profileDir")
        val soFile = builtLibDir.listFiles()?.firstOrNull {
            it.isFile && it.name.startsWith("lib") && it.name.endsWith(".so")
        } ?: throw GradleException("No .so built under ${builtLibDir.absolutePath}")

        val jniOutDir = File(project.projectDir, "src/main/jniLibs/$abi")
        jniOutDir.mkdirs()
        soFile.copyTo(File(jniOutDir, soFile.name), overwrite = true)
    }

    private fun runTauriCli(executable: String) {
        val rootDirRel = rootDirRel ?: throw GradleException("rootDirRel cannot be null")
        val target = target ?: throw GradleException("target cannot be null")
        val release = release ?: throw GradleException("release cannot be null")
        val args = listOf("tauri", "android", "android-studio-script")

        project.exec {
            workingDir(File(project.projectDir, rootDirRel))
            executable(executable)
            args(args)
            if (project.logger.isEnabled(LogLevel.DEBUG)) {
                args("-vv")
            } else if (project.logger.isEnabled(LogLevel.INFO)) {
                args("-v")
            }
            if (release) {
                args("--release")
            }
            args(listOf("--target", target))
        }.assertNormalExitValue()
    }
}

