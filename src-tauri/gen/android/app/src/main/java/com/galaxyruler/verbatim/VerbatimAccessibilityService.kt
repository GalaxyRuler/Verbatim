package com.galaxyruler.verbatim

import android.accessibilityservice.AccessibilityService
import android.os.Bundle
import android.text.InputType
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo

class VerbatimAccessibilityService : AccessibilityService() {
  private var focusedNode: AccessibilityNodeInfo? = null

  override fun onServiceConnected() {
    instance = this
  }

  override fun onAccessibilityEvent(event: AccessibilityEvent?) {
    val source = event?.source ?: return
    if (event.eventType == AccessibilityEvent.TYPE_VIEW_FOCUSED ||
      event.eventType == AccessibilityEvent.TYPE_VIEW_TEXT_SELECTION_CHANGED
    ) {
      if (isEditableNode(source)) {
        focusedNode = AccessibilityNodeInfo.obtain(source)
      }
    }
  }

  override fun onInterrupt() = Unit

  override fun onDestroy() {
    if (instance === this) {
      instance = null
    }
    focusedNode?.recycle()
    focusedNode = null
    super.onDestroy()
  }

  private fun insertText(text: CharSequence): InsertResult {
    val target = rootInActiveWindow?.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
      ?: focusedNode
      ?: return InsertResult.NO_TARGET

    if (!isEditableNode(target)) {
      return InsertResult.NO_TARGET
    }

    if (isSensitiveNode(target)) {
      return InsertResult.SENSITIVE
    }

    val args = Bundle().apply {
      putCharSequence(
        AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE,
        text,
      )
    }

    return if (target.performAction(AccessibilityNodeInfo.ACTION_SET_TEXT, args)) {
      InsertResult.INSERTED
    } else {
      InsertResult.FAILED
    }
  }

  private fun isEditableNode(node: AccessibilityNodeInfo): Boolean {
    if (node.isEditable) {
      return true
    }
    val className = node.className?.toString()?.lowercase() ?: return false
    return className.contains("edittext") || className.contains("textfield")
  }

  private fun isSensitiveNode(node: AccessibilityNodeInfo): Boolean {
    if (node.isPassword) {
      return true
    }

    val inputType = node.inputType
    val variation = inputType and InputType.TYPE_MASK_VARIATION
    return variation == InputType.TYPE_TEXT_VARIATION_PASSWORD ||
      variation == InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD ||
      variation == InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD ||
      variation == InputType.TYPE_NUMBER_VARIATION_PASSWORD
  }

  enum class InsertResult {
    INSERTED,
    FAILED,
    NO_TARGET,
    SENSITIVE,
  }

  companion object {
    @Volatile
    private var instance: VerbatimAccessibilityService? = null

    fun isEnabled(): Boolean = instance != null

    fun insert(text: CharSequence): InsertResult =
      instance?.insertText(text) ?: InsertResult.NO_TARGET
  }
}
