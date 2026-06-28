package com.galaxyruler.verbatim

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidLlmPostProcessorSupportTest {
  @Test
  fun highEndArm64DevicePassesGate() {
    val snapshot = AndroidLlmPostProcessorSupport.evaluate(
      AndroidLlmDeviceSpec(
        totalRamMb = 11_184,
        availableRamMb = 4_720,
        supported64BitAbis = listOf("arm64-v8a"),
        hardware = "qcom",
        socModel = "SM8550",
      ),
    )

    assertTrue(snapshot.supported)
    assertEquals("supported", snapshot.reason)
  }

  @Test
  fun lowRamDeviceIsUnsupported() {
    val snapshot = AndroidLlmPostProcessorSupport.evaluate(
      AndroidLlmDeviceSpec(
        totalRamMb = 6144,
        availableRamMb = 2500,
        supported64BitAbis = listOf("arm64-v8a"),
        hardware = "qcom",
        socModel = "SM8550",
      ),
    )

    assertFalse(snapshot.supported)
    assertEquals("requires8GbRam", snapshot.reason)
  }

  @Test
  fun nonArm64DeviceIsUnsupported() {
    val snapshot = AndroidLlmPostProcessorSupport.evaluate(
      AndroidLlmDeviceSpec(
        totalRamMb = 16_384,
        availableRamMb = 8192,
        supported64BitAbis = listOf("x86_64"),
        hardware = "ranchu",
        socModel = "",
      ),
    )

    assertFalse(snapshot.supported)
    assertEquals("requiresArm64", snapshot.reason)
  }

  @Test
  fun unknownMidrangeSocIsUnsupportedEvenWithEightGb() {
    val snapshot = AndroidLlmPostProcessorSupport.evaluate(
      AndroidLlmDeviceSpec(
        totalRamMb = 8192,
        availableRamMb = 4096,
        supported64BitAbis = listOf("arm64-v8a"),
        hardware = "qcom",
        socModel = "SM6375",
      ),
    )

    assertFalse(snapshot.supported)
    assertEquals("requiresHighEndSoc", snapshot.reason)
  }
}
