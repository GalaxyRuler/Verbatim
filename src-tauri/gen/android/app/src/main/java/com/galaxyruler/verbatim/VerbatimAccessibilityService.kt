package com.galaxyruler.verbatim

import android.accessibilityservice.AccessibilityService
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.text.InputType
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo
import android.view.accessibility.AccessibilityWindowInfo

class VerbatimAccessibilityService : AccessibilityService() {
  private var focusedNode: AccessibilityNodeInfo? = null
  private val mainHandler = Handler(Looper.getMainLooper())
  private val shortRefreshRunnable = Runnable { refreshFocusedNode() }
  private val longRefreshRunnable = Runnable { refreshFocusedNode() }

  override fun onServiceConnected() {
    instance = this
  }

  override fun onAccessibilityEvent(event: AccessibilityEvent?) {
    event ?: return
    when (event.eventType) {
      AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED,
      AccessibilityEvent.TYPE_WINDOWS_CHANGED -> {
        refreshFocusedNode()
        scheduleBubbleRefresh()
      }
      AccessibilityEvent.TYPE_VIEW_FOCUSED -> {
        val source = event.source ?: return
        if (isEditableNode(source)) {
          setFocusedNode(source)
          scheduleBubbleRefresh()
        } else {
          clearFocusedNode()
        }
      }
      AccessibilityEvent.TYPE_VIEW_TEXT_SELECTION_CHANGED -> {
        val source = event.source ?: return
        if (isEditableNode(source)) {
          setFocusedNode(source)
          scheduleBubbleRefresh()
        }
      }
    }
  }

  override fun onInterrupt() = Unit

  override fun onDestroy() {
    if (instance === this) {
      instance = null
    }
    mainHandler.removeCallbacks(shortRefreshRunnable)
    mainHandler.removeCallbacks(longRefreshRunnable)
    clearFocusedNode()
    super.onDestroy()
  }

  private fun insertText(text: CharSequence): InsertResult {
    val target = findFocusedEditableNode()
      ?: return InsertResult.NO_TARGET

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

  private fun findFocusedEditableNode(): AccessibilityNodeInfo? {
    rootInActiveWindow
      ?.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
      ?.takeIf { isEditableNode(it) }
      ?.let { return it }

    focusedNode
      ?.takeIf { it.refresh() && isEditableNode(it) }
      ?.let { return it }

    windows.forEach { window ->
      window.root
        ?.findFocus(AccessibilityNodeInfo.FOCUS_INPUT)
        ?.takeIf { isEditableNode(it) }
        ?.let { return it }
    }

    return null
  }

  private fun refreshFocusedNode() {
    val target = findFocusedEditableNode()
    if (target == null) {
      clearFocusedNode()
    } else {
      setFocusedNode(target)
    }
  }

  private fun scheduleBubbleRefresh() {
    mainHandler.removeCallbacks(shortRefreshRunnable)
    mainHandler.removeCallbacks(longRefreshRunnable)
    mainHandler.postDelayed(shortRefreshRunnable, 250)
    mainHandler.postDelayed(longRefreshRunnable, 900)
  }

  private fun setFocusedNode(node: AccessibilityNodeInfo) {
    val next = AccessibilityNodeInfo.obtain(node)
    focusedNode?.recycle()
    focusedNode = next
    updateBubbleTargetActive()
  }

  private fun clearFocusedNode() {
    focusedNode?.recycle()
    focusedNode = null
    FloatingBubbleService.setInputTargetActive(this, false)
  }

  private fun updateBubbleTargetActive() {
    val target = focusedNode
    val active = target != null &&
      target.refresh() &&
      isEditableNode(target) &&
      isInputMethodVisible()
    FloatingBubbleService.setInputTargetActive(this, active)
  }

  private fun isInputMethodVisible(): Boolean =
    windows.any { window -> window.type == AccessibilityWindowInfo.TYPE_INPUT_METHOD }

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
