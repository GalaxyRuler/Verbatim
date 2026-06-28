package com.galaxyruler.verbatim

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidLlmPostProcessorTest {
  @Test
  fun cleanupPromptPreservesLanguageAndForbidsTranslation() {
    val prompt = AndroidLlmPostProcessor.buildCleanupPrompt("hello comma خالد")

    assertTrue(prompt.contains("Do not translate"))
    assertTrue(prompt.contains("Preserve every language and script"))
    assertTrue(prompt.contains("Return only the cleaned transcript"))
  }

  @Test
  fun acceptsLightCleanupWhenTermsAndScriptsRemain() {
    assertEquals(
      "Hello, خالد.",
      AndroidLlmPostProcessor.normalizeModelOutput("hello comma خالد", "Hello, خالد."),
    )
  }

  @Test
  fun rejectsBlankOrUnchangedOutputForRawFallback() {
    assertNull(AndroidLlmPostProcessor.normalizeModelOutput("hello", ""))
    assertNull(AndroidLlmPostProcessor.normalizeModelOutput("hello", "hello"))
  }

  @Test
  fun rejectsOutputsThatDropSourceTerms() {
    assertNull(
      AndroidLlmPostProcessor.normalizeModelOutput(
        "email signature",
        "Regards,\nAbdullah",
      ),
    )
  }

  @Test
  fun rejectsOutputsThatLoseOriginalScript() {
    assertNull(
      AndroidLlmPostProcessor.normalizeModelOutput(
        "please keep пример as written",
        "Please keep primer as written.",
      ),
    )
  }
}
