package com.longbridge.nsh;

/**
 * Hears a scroll view the host watches ({@link NshScrollHandle#addScrollListener}),
 * on the UI thread, in the frame of the change.
 */
public interface NshScrollListener {
    /**
     * The vertical offset changed: the user's scroll, a fling, the app's or
     * the host's.
     *
     * @param scrollY the offset, in px
     * @param dy the change since the last call, in px (positive: the content
     *     moved up)
     */
    void onScroll(int scrollY, int dy);

    /**
     * The content got taller or shorter without a scroll (data arrived, a
     * section opened): what depends on where the content ends moves.
     *
     * @param contentHeight the content's height, in px (at least the
     *     viewport's)
     */
    default void onContentSizeChanged(int contentHeight) {}
}
