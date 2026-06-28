package com.galaxyruler.verbatim

import android.app.ActivityManager
import android.content.Context
import android.os.Build
import kotlin.math.roundToLong

data class AndroidLlmDeviceSpec(
  val totalRamMb: Long,
  val availableRamMb: Long,
  val supported64BitAbis: List<String>,
  val hardware: String,
  val socModel: String,
)

data class AndroidLlmSupportSnapshot(
  val supported: Boolean,
  val reason: String,
  val totalRamMb: Long,
  val availableRamMb: Long,
  val minRamMb: Long,
  val hardware: String,
  val socModel: String,
)

object AndroidLlmPostProcessorSupport {
  const val MIN_TOTAL_RAM_MB = 8192L
  private const val HIGH_RAM_FALLBACK_MB = 12288L
  private const val BYTES_PER_MB = 1024.0 * 1024.0

  fun snapshot(context: Context): AndroidLlmSupportSnapshot =
    evaluate(readDeviceSpec(context))

  fun evaluate(spec: AndroidLlmDeviceSpec): AndroidLlmSupportSnapshot {
    val hasArm64 = spec.supported64BitAbis.any { it.equals("arm64-v8a", ignoreCase = true) }
    val enoughRam = spec.totalRamMb >= MIN_TOTAL_RAM_MB
    val highEndSoc = isHighEndSoc(spec) ||
      (spec.totalRamMb >= HIGH_RAM_FALLBACK_MB && spec.hardware.isNotBlank())

    val reason = when {
      !hasArm64 -> "requiresArm64"
      !enoughRam -> "requires8GbRam"
      !highEndSoc -> "requiresHighEndSoc"
      else -> "supported"
    }

    return AndroidLlmSupportSnapshot(
      supported = reason == "supported",
      reason = reason,
      totalRamMb = spec.totalRamMb,
      availableRamMb = spec.availableRamMb,
      minRamMb = MIN_TOTAL_RAM_MB,
      hardware = spec.hardware,
      socModel = spec.socModel,
    )
  }

  private fun readDeviceSpec(context: Context): AndroidLlmDeviceSpec {
    val memoryInfo = ActivityManager.MemoryInfo()
    context.getSystemService(ActivityManager::class.java)?.getMemoryInfo(memoryInfo)
    return AndroidLlmDeviceSpec(
      totalRamMb = bytesToMb(memoryInfo.totalMem),
      availableRamMb = bytesToMb(memoryInfo.availMem),
      supported64BitAbis = Build.SUPPORTED_64_BIT_ABIS.toList(),
      hardware = Build.HARDWARE.orEmpty(),
      socModel = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
        Build.SOC_MODEL.orEmpty()
      } else {
        ""
      },
    )
  }

  private fun bytesToMb(bytes: Long): Long =
    (bytes / BYTES_PER_MB).roundToLong()

  private fun isHighEndSoc(spec: AndroidLlmDeviceSpec): Boolean {
    val soc = spec.socModel.uppercase()
    val hardware = spec.hardware.uppercase()
    return listOf("SM8550", "SM8650", "SM8750", "SM8850").any { it in soc } ||
      listOf("TENSOR G3", "TENSOR G4", "TENSOR G5").any { it in soc } ||
      listOf("MT6989", "MT6991", "MT6993").any { it in soc } ||
      (hardware == "QCOM" && spec.totalRamMb >= HIGH_RAM_FALLBACK_MB)
  }
}
