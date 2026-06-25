package com.galaxyruler.verbatim

object PcmFrameNormalizer {
  const val SAMPLE_RATE = 16_000
  const val FRAME_SIZE = 512

  fun pcm16LeToFloatFrames(bytes: ByteArray): List<FloatArray> {
    val sampleCount = bytes.size / 2
    if (sampleCount == 0) {
      return emptyList()
    }

    val frames = mutableListOf<FloatArray>()
    var sampleIndex = 0
    while (sampleIndex < sampleCount) {
      val frameSize = minOf(FRAME_SIZE, sampleCount - sampleIndex)
      val frame = FloatArray(frameSize)
      for (offset in 0 until frameSize) {
        val byteIndex = (sampleIndex + offset) * 2
        val lo = bytes[byteIndex].toInt() and 0xff
        val hi = bytes[byteIndex + 1].toInt()
        val sample = ((hi shl 8) or lo).toShort()
        frame[offset] = sample.toFloat() / 32768f
      }
      frames.add(frame)
      sampleIndex += frameSize
    }

    return frames
  }
}
