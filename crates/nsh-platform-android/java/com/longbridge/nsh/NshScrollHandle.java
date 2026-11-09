package com.longbridge.nsh;

import android.view.View;

/**
 * A {@code <Scroll>} of the app, for native UI of the host that follows or
 * drives the app's vertical scrolling in the frame of the scroll, without a
 * round trip through JS: a sheet under the content that the end of the
 * content pulls up, say. Get one from {@link NshView#watchScroll}. Lengths
 * are px; call it on the UI thread.
 *
 * <p>Once the scroll view is deleted, loses its {@code testID} or its app
 * stops, {@link #isAlive()} is false, reads give 0 and the rest does nothing.
 */
public final class NshScrollHandle {
    private final ScrollBox box;
    private final String key;

    NshScrollHandle(ScrollBox box, String key) {
        this.box = box;
        this.key = key;
    }

    /** Whether the scroll view is still the app's, with the watched {@code testID}. */
    public boolean isAlive() {
        return !box.unmounted && key.equals(box.hostKey);
    }

    /** The vertical offset. */
    public int getScrollY() {
        return isAlive() ? box.hostScrollY() : 0;
    }

    /** The content's height (at least the viewport's). */
    public int getContentHeight() {
        return isAlive() ? box.hostContentHeight() : 0;
    }

    /** The visible height. */
    public int getViewportHeight() {
        return isAlive() ? box.hostViewportHeight() : 0;
    }

    /**
     * The view holding the content: the positions of the views inside are
     * relative to it. Null once the scroll view is gone.
     */
    public View getContentView() {
        return isAlive() ? box.content : null;
    }

    /**
     * Scrolls by {@code dy} at once, clamped to the content, as the user's
     * scroll would (the app's {@code onScroll} hears it).
     */
    public void scrollBy(int dy) {
        if (isAlive()) box.hostScrollBy(dy, false);
    }

    /** Scrolls by {@code dy} with the platform's animation. */
    public void smoothScrollBy(int dy) {
        if (isAlive()) box.hostScrollBy(dy, true);
    }

    /** Ends a fling or a smooth scroll where it is; nothing when at rest. */
    public void stopFling() {
        if (isAlive()) box.hostStop();
    }

    /** Calls {@code listener} on every offset and content size change, once per listener. */
    public void addScrollListener(NshScrollListener listener) {
        if (isAlive() && !box.hostListeners.contains(listener)) box.hostListeners.add(listener);
    }

    /** Stops calling {@code listener}. */
    public void removeScrollListener(NshScrollListener listener) {
        box.hostListeners.remove(listener);
    }
}
