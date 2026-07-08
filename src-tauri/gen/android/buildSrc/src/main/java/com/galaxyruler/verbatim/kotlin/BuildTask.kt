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
    var features: String? = null
    @Input
    var target: String? = null
    @Input
    var release: Boolean? = null

    @TaskAction
    fun assemble() {
        val executable = if (features.isNullOrBlank()) """bun""" else cargoExecutable()
        try {
            runBuild(executable)
        } catch (e: Exception) {
            if (Os.isFamily(Os.FAMILY_WINDOWS) && !File(executable).isAbsolute) {
                // Try different Windows-specific extensions
                val fallbacks = listOf(
                    "$executable.exe",
                    "$executable.cmd",
                    "$executable.bat",
                )

                var lastException: Exception = e
                for (fallback in fallbacks) {
                    try {
                        runBuild(fallback)
                        return
                    } catch (fallbackException: Exception) {
                        lastException = fallbackException
                    }
                }
                throw lastException
            } else {
                throw e;
            }
        }
    }

    fun runBuild(executable: String) {
        if (features.isNullOrBlank()) {
            runTauriCli(executable)
        } else {
            runCargoBuild(executable)
        }
    }

    fun runTauriCli(executable: String) {
        val rootDirRel = rootDirRel ?: throw GradleException("rootDirRel cannot be null")
        val target = target ?: throw GradleException("target cannot be null")
        val release = release ?: throw GradleException("release cannot be null")
        val args = listOf("tauri", "android", "android-studio-script");

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

    fun runCargoBuild(executable: String) {
        val rootDirRel = rootDirRel ?: throw GradleException("rootDirRel cannot be null")
        val target = target ?: throw GradleException("target cannot be null")
        val release = release ?: throw GradleException("release cannot be null")
        val features = features?.trim() ?: throw GradleException("features cannot be null")
        val (rustTarget, androidAbi) = androidRustTarget(target)
        val cargoProfile = if (release) "release" else "debug"
        val rootDir = File(project.projectDir, rootDirRel)
        val sherpaOnnxLibDir = sherpaOnnxLibDirForTarget(rustTarget, androidAbi)
        // Release libraries must serve the bundled frontend, not build.devUrl.
        // The generated Gradle feature list only carries app features, so enable
        // tauri/custom-protocol here and null out devUrl at Rust compile time.
        val effectiveFeatures = if (release) releaseFeatures(features) else features

        project.exec {
            workingDir(rootDir)
            executable(executable)
            androidCcEnv(rustTarget)?.let { (ccEnv, ccPath) -> environment(ccEnv, ccPath) }
            androidCxxEnv(rustTarget)?.let { (cxxEnv, cxxPath) -> environment(cxxEnv, cxxPath) }
            androidArEnv(rustTarget)?.let { (arEnv, arPath) -> environment(arEnv, arPath) }
            androidRanlibEnv(rustTarget)?.let { (ranlibEnv, ranlibPath) -> environment(ranlibEnv, ranlibPath) }
            sherpaOnnxLibDir?.let { environment("SHERPA_ONNX_LIB_DIR", it) }
            environment("SHERPA_ONNX_ANDROID_ABI", androidAbi)
            args("build", "--lib", "--target", rustTarget, "--features", effectiveFeatures)
            if (release) {
                environment("TAURI_CONFIG", releaseTauriConfig())
                args("--release")
            }
        }.assertNormalExitValue()

        val targetRoot = System.getenv("CARGO_TARGET_DIR")
            ?.takeIf { it.isNotBlank() }
            ?.let { File(it) }
            ?: File(rootDir, "target")
        val builtLibrary = File(targetRoot, "$rustTarget/$cargoProfile/libverbatim_app_lib.so")
        if (!builtLibrary.isFile) {
            throw GradleException("Rust library was not built at ${builtLibrary.absolutePath}")
        }

        val jniLibDir = File(project.projectDir, "src/main/jniLibs/$androidAbi")
        jniLibDir.mkdirs()
        builtLibrary.copyTo(File(jniLibDir, "libverbatim_app_lib.so"), overwrite = true)
    }

    private fun androidRustTarget(target: String): Pair<String, String> =
        when (target) {
            "aarch64" -> "aarch64-linux-android" to "arm64-v8a"
            "armv7" -> "armv7-linux-androideabi" to "armeabi-v7a"
            "i686" -> "i686-linux-android" to "x86"
            "x86_64" -> "x86_64-linux-android" to "x86_64"
            else -> throw GradleException("Unsupported Android Rust target: $target")
        }

    private fun releaseFeatures(features: String): String {
        val list = features.split(Regex("[,\\s]+")).filter { it.isNotBlank() }
        if (RELEASE_BUNDLED_FRONTEND_FEATURE in list) {
            return features
        }
        return (list + RELEASE_BUNDLED_FRONTEND_FEATURE).joinToString(" ")
    }

    // tauri-build/tauri-codegen merge the TAURI_CONFIG env var (JSON merge patch)
    // into tauri.conf.json at compile time; a null devUrl removes the key, so the
    // dev-server URL never reaches the release library.
    private fun releaseTauriConfig(): String {
        val existing = System.getenv("TAURI_CONFIG")
        if (existing.isNullOrBlank()) {
            return """{"build":{"devUrl":null}}"""
        }
        @Suppress("UNCHECKED_CAST")
        val root = groovy.json.JsonSlurper().parseText(existing) as? MutableMap<Any?, Any?>
            ?: throw GradleException("TAURI_CONFIG must be a JSON object so release builds can null out build.devUrl")
        @Suppress("UNCHECKED_CAST")
        val build = root["build"] as? MutableMap<Any?, Any?>
            ?: mutableMapOf<Any?, Any?>().also { root["build"] = it }
        build["devUrl"] = null
        return groovy.json.JsonOutput.toJson(root)
    }

    private fun sherpaOnnxLibDirForTarget(rustTarget: String, androidAbi: String): String? {
        val rustTargetEnv = "SHERPA_ONNX_LIB_DIR_${rustTarget.uppercase().replace('-', '_')}"
        val androidAbiEnv = "SHERPA_ONNX_LIB_DIR_${androidAbi.uppercase().replace('-', '_')}"
        return listOf(rustTargetEnv, androidAbiEnv, "SHERPA_ONNX_LIB_DIR")
            .asSequence()
            .mapNotNull { System.getenv(it) }
            .firstOrNull { it.isNotBlank() }
    }

    private fun cargoExecutable(): String {
        System.getenv("CARGO")
            ?.takeIf { it.isNotBlank() }
            ?.let { return it }

        val cargoHome = System.getenv("CARGO_HOME")
            ?.takeIf { it.isNotBlank() }
            ?.let { File(it) }
            ?: File(System.getProperty("user.home"), ".cargo")
        val cargo = File(cargoHome, "bin/${ndkToolName("cargo")}")
        if (cargo.isFile) {
            return cargo.absolutePath
        }

        return "cargo"
    }

    private fun androidCcEnv(rustTarget: String): Pair<String, String>? {
        val linker = androidClang(rustTarget) ?: return null
        return "CC_${rustTarget.replace('-', '_')}" to linker
    }

    private fun androidCxxEnv(rustTarget: String): Pair<String, String>? {
        val linker = androidClang(rustTarget) ?: return null
        val cxx = when {
            linker.endsWith("clang.cmd") -> linker.removeSuffix("clang.cmd") + "clang++.cmd"
            linker.endsWith("clang") -> linker.removeSuffix("clang") + "clang++"
            else -> linker
        }
        return "CXX_${rustTarget.replace('-', '_')}" to cxx
    }

    private fun androidArEnv(rustTarget: String): Pair<String, String>? {
        val toolchainBin = androidToolchainBin(rustTarget) ?: return null
        return "AR_${rustTarget.replace('-', '_')}" to File(toolchainBin, ndkToolName("llvm-ar")).absolutePath
    }

    private fun androidRanlibEnv(rustTarget: String): Pair<String, String>? {
        val toolchainBin = androidToolchainBin(rustTarget) ?: return null
        return "RANLIB_${rustTarget.replace('-', '_')}" to File(toolchainBin, ndkToolName("llvm-ranlib")).absolutePath
    }

    private fun androidClang(rustTarget: String): String? {
        System.getenv(cargoTargetLinkerEnv(rustTarget))
            ?.takeIf { it.isNotBlank() }
            ?.let { return it }

        val toolchainBin = androidToolchainBin(rustTarget) ?: return null
        return File(
            toolchainBin,
            "${androidClangPrefix(rustTarget)}$ANDROID_API_LEVEL-${ndkToolName("clang")}",
        ).absolutePath
    }

    private fun androidToolchainBin(rustTarget: String): File? {
        System.getenv(cargoTargetLinkerEnv(rustTarget))
            ?.takeIf { it.isNotBlank() }
            ?.let { return File(it).parentFile }

        val ndkHome = explicitNdkHome()
            ?: latestSdkNdkHome()
            ?: return null
        val host = when {
            Os.isFamily(Os.FAMILY_WINDOWS) -> "windows-x86_64"
            Os.isFamily(Os.FAMILY_MAC) -> "darwin-x86_64"
            else -> "linux-x86_64"
        }
        return File(ndkHome, "toolchains/llvm/prebuilt/$host/bin")
    }

    private fun explicitNdkHome(): File? =
        listOf("ANDROID_NDK_HOME", "NDK_HOME")
            .asSequence()
            .mapNotNull { System.getenv(it) }
            .map { File(it) }
            .firstOrNull { it.isDirectory }

    private fun latestSdkNdkHome(): File? =
        listOf("ANDROID_HOME", "ANDROID_SDK_ROOT")
            .asSequence()
            .mapNotNull { System.getenv(it) }
            .map { File(it, "ndk") }
            .flatMap { it.listFiles()?.asSequence() ?: emptySequence() }
            .filter { it.isDirectory }
            .maxByOrNull { it.name }

    private fun androidClangPrefix(rustTarget: String): String =
        when (rustTarget) {
            "aarch64-linux-android" -> "aarch64-linux-android"
            "armv7-linux-androideabi" -> "armv7a-linux-androideabi"
            "i686-linux-android" -> "i686-linux-android"
            "x86_64-linux-android" -> "x86_64-linux-android"
            else -> throw GradleException("Unsupported Android Rust target: $rustTarget")
        }

    private fun ndkToolName(name: String): String =
        when {
            Os.isFamily(Os.FAMILY_WINDOWS) && name == "clang" -> "$name.cmd"
            Os.isFamily(Os.FAMILY_WINDOWS) -> "$name.exe"
            else -> name
        }

    private fun cargoTargetLinkerEnv(rustTarget: String): String =
        "CARGO_TARGET_${rustTarget.uppercase().replace('-', '_')}_LINKER"

    private companion object {
        const val ANDROID_API_LEVEL = 26
        const val RELEASE_BUNDLED_FRONTEND_FEATURE = "tauri/custom-protocol"
    }
}
