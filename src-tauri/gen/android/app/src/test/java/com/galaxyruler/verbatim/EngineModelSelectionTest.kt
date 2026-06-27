package com.galaxyruler.verbatim

import android.content.Context
import android.content.SharedPreferences
import io.mockk.Runs
import io.mockk.every
import io.mockk.just
import io.mockk.mockk
import org.junit.Assert.assertEquals
import org.junit.Test

class EngineModelSelectionTest {
  @Test
  fun setEngineModelIdPersistsSelectedPackAndNormalizesBlankValues() {
    val context = mockk<Context>()
    val prefs = mockk<SharedPreferences>()
    val editor = mockk<SharedPreferences.Editor>()
    var storedModelId: String? = null

    every { context.getSharedPreferences("verbatim_android", Context.MODE_PRIVATE) } returns prefs
    every { prefs.getString("native_engine_model_id", "default") } answers {
      storedModelId ?: "default"
    }
    every { prefs.edit() } returns editor
    every { editor.putString("native_engine_model_id", any()) } answers {
      storedModelId = secondArg()
      editor
    }
    every { editor.apply() } just Runs

    assertEquals(
      "g3-zipformer-whisper-tiny-en",
      EngineModelSelectionStore.setEngineModelId(context, " g3-zipformer-whisper-tiny-en "),
    )
    assertEquals("g3-zipformer-whisper-tiny-en", EngineModelSelectionStore.engineModelId(context))

    assertEquals("default", EngineModelSelectionStore.setEngineModelId(context, " "))
    assertEquals("default", EngineModelSelectionStore.engineModelId(context))
  }
}
