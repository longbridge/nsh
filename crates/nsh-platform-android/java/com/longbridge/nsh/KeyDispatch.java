package com.longbridge.nsh;

import android.util.SparseArray;
import android.view.KeyEvent;
import android.view.View;

/**
 * The keyboard of one native window: the app's shortcuts, the focused
 * managed view's raw {@code onKeyDown} / {@code onKeyUp}, its mounted
 * {@code captureKeys} rules, and Tab / Shift+Tab focus traversal.
 *
 * <p>{@code NshView} is one window; a {@code ModalHost} dialog is another,
 * with its own focus and its own fallback root node. An {@code EditText} or
 * hosted native view keeps its own keys ({@link Mounter#keyNode} returns
 * `0`), and a {@code captureKeys} rule never reaches it. Decisions are made
 * synchronously on the UI thread from mounted state; a release uses the
 * focus, listeners and capture rules of the moment it arrives.
 *
 * <p>A system key ({@code KeyEvent.isSystem}: Back, Home, volume, media,
 * camera, call) is never captured, not even by {@code captureKeys: "all"}:
 * Android keeps its default. The raw {@code onKeyDown} / {@code onKeyUp}
 * observation is unaffected. A captured key is still observed, with its
 * {@code defaultPrevented} decision; observation and consumption are
 * separate.
 *
 * <p>Tab and Shift+Tab move between the managed stops and inputs in tree
 * order. At the first or last stop they yield to Android's own focus and
 * default handling instead of wrapping, so an app never traps Tab.
 *
 * <p>An uncaptured Return or Space down reaches Android's native pressed
 * state. If a repeat or release becomes captured, the native state is
 * cancelled with a `FLAG_CANCELED` up, so no stale press and no click
 * remain.
 */
final class KeyDispatch {
    private final Mounter mounter;
    /** The window's view tree, whose focus is the keyboard's ({@code NshView}'s root or a modal's container). */
    private final View root;
    /** The node a key goes to when nothing in this window has a focus ({@code ROOT_ID} or a modal's id). */
    private final int fallback;
    /** A Tab press moved the managed focus; its release is consumed too. */
    private boolean tabMoved;
    /** A down reached native handling: the view whose pressed state a captured repeat/up must cancel. */
    private final SparseArray<View> nativeDown = new SparseArray<>();
    /** A native press already cancelled while its key stayed held: its real up is cancelled too. */
    private final SparseArray<View> nativeCancelled = new SparseArray<>();

    KeyDispatch(Mounter mounter, View root, int fallback) {
        this.mounter = mounter;
        this.root = root;
        this.fallback = fallback;
    }

    boolean dispatchKeyEvent(KeyEvent e) {
        if (mounter.handle() == 0) return false;
        int action = e.getAction();
        if (action == KeyEvent.ACTION_DOWN) {
            if (e.getRepeatCount() == 0) {
                // A fresh press: any lost release state of this code is stale.
                nativeDown.remove(e.getKeyCode());
                nativeCancelled.remove(e.getKeyCode());
            }
            if (keyEvent(e)) {
                // A repeat or a re-down consumed the key after native
                // handling saw its first down: clear that native press
                // without its click.
                cancelNative(e);
                return true;
            }
            boolean tab = isTab(e);
            if (tab && mounter.moveFocus(root, focus(), !e.isShiftPressed())) {
                tabMoved = true;
                return true;
            }
            // Not a Tab, or a Tab the stops yielded at an end: its release
            // belongs to Android too, not to an earlier move of this press.
            tabMoved = false;
            nativeCancelled.remove(e.getKeyCode());
            View focus = focus();
            if (focus != null && focus != root && mounter.keyNode(focus, fallback) > 1
                    && nativeDown.get(e.getKeyCode()) == null) {
                nativeDown.put(e.getKeyCode(), focus);
            }
            return false;
        }
        if (action == KeyEvent.ACTION_UP) {
            boolean consumed = keyEvent(e);
            View cancelled = nativeCancelled.get(e.getKeyCode());
            if (cancelled != null) {
                nativeCancelled.remove(e.getKeyCode());
                cancelNative(cancelled, e);
                return true;
            }
            boolean moved = false;
            if (tabMoved && e.getKeyCode() == KeyEvent.KEYCODE_TAB) {
                tabMoved = false;
                moved = true;
            }
            boolean result = consumed || moved;
            View nativeView = nativeDown.get(e.getKeyCode());
            if (nativeView != null) {
                nativeDown.remove(e.getKeyCode());
                if (result || nativeView != focus()) cancelNative(nativeView, e);
            }
            return result;
        }
        return false;
    }

    boolean dispatchKeyShortcutEvent(KeyEvent e) {
        if (mounter.handle() == 0 || e.getAction() != KeyEvent.ACTION_DOWN) return false;
        View focus = focus();
        int node = mounter.keyNode(focus, fallback);
        return Native.keyShortcut(mounter.handle(), e.getKeyCode(), e.getUnicodeChar(0),
                e.getMetaState(), fallback, mounter.control(node), e.getRepeatCount() > 0,
                node == 0);
    }

    /** The window lost its focus or goes away: no held key has a release here. */
    void focusLost() {
        tabMoved = false;
        nativeDown.clear();
        nativeCancelled.clear();
    }

    private View focus() {
        return root.findFocus();
    }

    private boolean keyEvent(KeyEvent e) {
        View focus = focus();
        int node = mounter.keyNode(focus, fallback);
        // An input or hosted native view has the focus: it is in text, and
        // its keys stay its own.
        boolean editing = node == 0;
        int code = e.getKeyCode(), meta = e.getMetaState();
        int unicode = e.getUnicodeChar(0);
        int text = sourceText(e);
        int scan = e.getScanCode();
        if (e.getAction() == KeyEvent.ACTION_UP) {
            if (e.isCanceled()) return Native.keyCancel(mounter.handle(), code);
            // The release uses the current focus, listeners and capture
            // rules, not what its down happened to do.
            boolean raw = node > 0 && mounter.keyListens(node, fallback);
            boolean prevented = raw && mounter.keyCaptured(node, fallback, e);
            return Native.keyUp(mounter.handle(), code, unicode, text, meta, scan, node,
                    prevented, raw);
        }
        boolean raw = node > 0 && mounter.keyListens(node, fallback);
        boolean prevented = raw && mounter.keyCaptured(node, fallback, e);
        return Native.keyDown(mounter.handle(), code, unicode, text, meta, scan,
                e.getRepeatCount() > 0, editing, node, fallback, mounter.control(node),
                prevented, raw);
    }

    /** Plain Tab or Shift+Tab: the app's focus traversal, not Ctrl/Alt/Meta + Tab. */
    private static boolean isTab(KeyEvent e) {
        return e.getKeyCode() == KeyEvent.KEYCODE_TAB
                && !e.isCtrlPressed() && !e.isAltPressed() && !e.isMetaPressed();
    }

    /**
     * The printable character the source reports with its modifiers
     * (Shift, Caps Lock, AltGr or Alt): the raw `key` when there is one. A
     * Control combination reports a control character; its Shift/Caps level
     * is the printable source key.
     */
    private static int sourceText(KeyEvent e) {
        int meta = e.getMetaState();
        int text = e.getUnicodeChar(meta);
        if (text == 0 || Character.isISOControl(text)) {
            // Control/Command produce a control character; Alt and AltGr
            // stay, so a composed source character survives.
            text = e.getUnicodeChar(meta & ~(KeyEvent.META_CTRL_ON | KeyEvent.META_META_ON));
        }
        if (text == 0 || Character.isISOControl(text)) {
            text = e.getUnicodeChar(meta
                    & (KeyEvent.META_SHIFT_ON | KeyEvent.META_CAPS_LOCK_ON));
        }
        return text != 0 ? text : e.getUnicodeChar(0);
    }

    private void cancelNative(KeyEvent e) {
        View view = nativeDown.get(e.getKeyCode());
        if (view != null) {
            nativeDown.remove(e.getKeyCode());
            if (e.getAction() == KeyEvent.ACTION_DOWN) {
                nativeCancelled.put(e.getKeyCode(), view);
            }
            cancelNative(view, e);
        }
    }

    /** Clears a view's native pressed state with a canceled up: no click. */
    private static void cancelNative(View view, KeyEvent e) {
        if (view.getWindowToken() == null) return;
        KeyEvent up = new KeyEvent(e.getDownTime(), e.getEventTime(), KeyEvent.ACTION_UP,
                e.getKeyCode(), 0, e.getMetaState(), e.getDeviceId(), e.getScanCode(),
                e.getFlags() | KeyEvent.FLAG_CANCELED, e.getSource());
        view.dispatchKeyEvent(up);
    }
}
