package com.galaxyruler.verbatim

import kotlin.math.abs
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidPcmFramingTest {
  @Test
  fun pcm16LittleEndianNormalizesToFloatFrames() {
    val bytes = pcm16le(-32768, 0, 32767)

    val frames = PcmFrameNormalizer.pcm16LeToFloatFrames(bytes)

    assertEquals(1, frames.size)
    assertEquals(3, frames.single().size)
    assertClose(-1.0f, frames.single()[0])
    assertClose(0.0f, frames.single()[1])
    assertClose(32767f / 32768f, frames.single()[2])
  }

  @Test
  fun pcm16LittleEndianChunksIntoSileroWindowSizedFrames() {
    val samples = IntArray(PcmFrameNormalizer.FRAME_SIZE * 2) { index ->
      if (index % 2 == 0) 32767 else -32768
    }

    val frames = PcmFrameNormalizer.pcm16LeToFloatFrames(pcm16le(*samples))

    assertEquals(2, frames.size)
    assertTrue(frames.all { it.size == PcmFrameNormalizer.FRAME_SIZE })
  }

  private fun pcm16le(vararg samples: Int): ByteArray {
    val bytes = ByteArray(samples.size * 2)
    samples.forEachIndexed { index, sample ->
      val coerced = sample.toShort().toInt()
      bytes[index * 2] = (coerced and 0xff).toByte()
      bytes[index * 2 + 1] = ((coerced shr 8) and 0xff).toByte()
    }
    return bytes
  }

  private fun assertClose(expected: Float, actual: Float) {
    assertTrue("expected $expected, got $actual", abs(expected - actual) < 0.0001f)
  }
}
