package com.galaxyruler.verbatim

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * Phase 0 / Task 0.1 — proves the JVM unit-test loop runs (JUnit4, the stack already
 * present in app/build.gradle.kts). No Android framework, no Rust/NDK dependency.
 */
class SmokeTest {
    @Test
    fun arithmetic_sanity() {
        assertEquals(4, 2 + 2)
    }
}
