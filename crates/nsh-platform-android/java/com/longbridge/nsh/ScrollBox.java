package com.longbridge.nsh;

import android.animation.ValueAnimator;
import android.annotation.SuppressLint;
import android.content.Context;
import android.graphics.Canvas;
import android.graphics.Rect;
import android.os.Build;
import android.view.MotionEvent;
import android.view.ScaleGestureDetector;
import android.view.VelocityTracker;
import android.view.View;
import android.view.ViewConfiguration;
import android.view.ViewGroup;
import android.view.ViewParent;
import android.view.WindowInsets;
import android.view.inputmethod.InputMethodManager;
import android.widget.FrameLayout;
import android.widget.HorizontalScrollView;
import android.widget.OverScroller;
import android.widget.ProgressBar;
import android.widget.ScrollView;

/**
 * `<scroll>` and lists: a platform ScrollView (native fling and overscroll
 * effect) whose content box is sized by the core, plus pull to refresh,
 * paging and snapping, drag / momentum events and
 * `keyboardShouldPersistTaps`.
 */
@SuppressLint("ViewConstructor")
final class ScrollBox extends FrameLayout implements Node {
    final Mounter mounter;
    final int id;
    final Decor decor = new Decor();

    /** An outer box shadow draws outside the bounds, and compositing the
     * view at alpha < 1 into the bounds-clipped offscreen buffer would cut
     * it; such a view reports no overlapping rendering, so the alpha
     * applies per draw op and the shadow fades with the content. */
    @Override
    public boolean hasOverlappingRendering() {
        return !decor.hasOuterShadows();
    }

    /** The events it listens to, kinds below 64 ({@link Mounter#wants} has all). */
    long events;
    boolean horizontal;
    int dismissMode; // 0 none, 1 on drag, 2 interactive
    private FrameLayout scroller;
    final Content content;
    private final ProgressBar spinner;
    private boolean refreshing;
    /** Content offset while pulling or refreshing (px). */
    private float pull;
    private boolean pulling;
    private float startY;
    private boolean touching;
    /** The down time of the touch whose DOWN this view took (-1: none yet). */
    private long downTime = -1;
    private final Runnable settled = this::settled;

    // `ScrollBehavior`; offsets and lengths in dp.
    private boolean paging, enabled = true, bounces = true, oneStep;
    /** A JS responder inside keeps the touches (`onResponderGrant` returned true). */
    boolean jsBlocked;
    /** The pan of a view inside activated (`panActivation`): it keeps the touches. */
    boolean gateBlocked;
    private float snapInterval, deceleration;
    private float[] snapOffsets = new float[0];
    private int snapAlignment; // 0 start, 1 center, 2 end
    private int keyboardTaps; // 0 never, 1 always, 2 handled
    /** `contentInset` (px) and the room kept for the keyboard below it. */
    private int insetTop, insetRight, insetBottom, insetLeft, keyboardPadding;

    // The current gesture.
    /** The user dragged the content. */
    private boolean dragging;
    /** The scroll view flung (or snapped) after the drag: gliding. */
    private boolean flung, momentum;
    /** This touch only dismisses the keyboard (`keyboardShouldPersistTaps`); -1: it does not. */
    private int tapMode = -1;
    /** Where this touch went down, and whether it moved past the slop since (a drag at an edge scrolls nothing). */
    private float tapX, tapY;
    private boolean tapMoved;
    /**
     * This view is dispatching a touch. The user's drag scrolls the content
     * synchronously inside that call (a nested scroll view hands it the rest
     * there too); commands, their glides, layout catching up (`payOwed`) and
     * the host's handle scroll outside it. Only a scroll inside it is the
     * user's drag, which takes the touches from the views inside.
     */
    private boolean dispatching;
    /**
     * Frames the platform scroll view drew. It draws again each frame while
     * its overscroll effect (Android 12's stretch) relaxes, which moves no
     * view: the test agent counts the frames as movement
     * ({@link TestAgent#animating}), since a touch caught then only stops
     * the stretch, and presses nothing.
     */
    int frames;

    // The host's handle (`NshScrollHandle`, `NshView.watchScroll`).
    /** The {@code nativeID} the host watches it by (null: none). */
    String hostKey;
    /** It was deleted, or its app stopped. */
    boolean unmounted;
    /** Hear every offset and content size change. */
    final java.util.List<NshScrollListener> hostListeners = new java.util.concurrent.CopyOnWriteArrayList<>();
    /** A smooth scroll of the host's may still be running. */
    private boolean hostAnimating;

    /**
     * The scrolled content, sized by the core (`ScrollContent`). In a
     * scroll view that does not zoom it draws only its children (rows) near
     * the viewport ({@link ScrollCull}): the rest stay laid out and attached,
     * so list binding, focus and accessibility see them as before. A row is
     * drawn wherever it is while it is translated, scaled, rotated or
     * mirrored, animates, or holds a descendant that pins it (see
     * {@link #inserted}): one transformed well past its own bounds, or an
     * app's native component.
     */
    static final class Content extends Box {
        int contentWidth, contentHeight;
        /** The viewport in the content's px (left, top, right, bottom). */
        private final int[] viewport = new int[4];
        /** While {@link #dispatchDraw} runs: whether it culls, with this
         * margin, and whether it records the display list. */
        private boolean culls, recording;
        private int margin;
        /** {@link #margin} for a scroll view of this size and density. */
        private int marginWidth = -1, marginHeight = -1;
        private float marginDensity;
        /**
         * What its display list was last recorded for: the viewport and
         * margin, and the children left out; {@code culled} is false once
         * that recording did not cull or is out of date (it draws again).
         */
        private final int[] drawn = new int[4];
        private int drawnMargin;
        private boolean culled;
        private final java.util.Set<View> skipped =
                java.util.Collections.newSetFromMap(new java.util.IdentityHashMap<>());
        /**
         * Rows kept drawn wherever they are, with the descendants that pin
         * them; a descendant pinned for its transform carries its row as tag
         * {@link #PIN}, so it is unpinned when the transform goes.
         */
        private final java.util.IdentityHashMap<View, java.util.ArrayList<View>> pins = new java.util.IdentityHashMap<>();
        /** Scratch for {@link #skips} and {@link #excess} (main thread). */
        private final float[] overflowScratch = new float[4], paintScratch = new float[4];
        private final int[] reachScratch = new int[4];
        private final android.graphics.RectF mapped = new android.graphics.RectF();

        /** Tag key of the row a descendant's transform pins (an id unlikely to clash, as `Effects.R_LAYER`). */
        private static final int PIN = 0x7f0e7f02;

        Content(Context context, Mounter mounter, int id) {
            super(context, mounter, id);
        }

        /**
         * Fills {@link #viewport}; false where it does not cull: a zooming
         * scroll view scales the content without drawing it again, and a
         * scroll view not laid out yet shows nothing to cull against. The
         * platform scroll views place the content at their padding and
         * never transform it.
         */
        private boolean viewport() {
            ViewParent parent = getParent();
            if (!(parent instanceof ScrollView || parent instanceof HorizontalScrollView)) return false;
            View scroller = (View) parent;
            int w = scroller.getWidth(), h = scroller.getHeight();
            if (w <= 0 || h <= 0) return false;
            int x = scroller.getScrollX() - getLeft(), y = scroller.getScrollY() - getTop();
            viewport[0] = x;
            viewport[1] = y;
            viewport[2] = x + w;
            viewport[3] = y + h;
            if (w != marginWidth || h != marginHeight || mounter.density != marginDensity) {
                marginWidth = w;
                marginHeight = h;
                marginDensity = mounter.density;
                margin = ScrollCull.margin(w, h, mounter.density);
            }
            return true;
        }

        @Override
        protected void dispatchDraw(Canvas canvas) {
            culls = viewport();
            // Only a hardware canvas records the display list that later
            // frames replay; a software drawing is drawn whole each time.
            recording = canvas.isHardwareAccelerated();
            if (recording) {
                skipped.clear();
                culled = culls;
                if (culls) {
                    System.arraycopy(viewport, 0, drawn, 0, 4);
                    drawnMargin = margin;
                }
            }
            try {
                super.dispatchDraw(canvas);
            } finally {
                culls = recording = false;
            }
        }

        @Override
        protected boolean drawChild(Canvas canvas, View child, long time) {
            if (culls && skips(child, viewport, margin)) {
                if (recording) skipped.add(child);
                return false;
            }
            return super.drawChild(canvas, child, time);
        }

        @Override
        public void onViewRemoved(View child) {
            super.onViewRemoved(child);
            // A row that left keeps nothing here; one inserted again is
            // scanned again (`inserted`).
            skipped.remove(child);
            java.util.ArrayList<View> by = pins.remove(child);
            if (by != null) {
                for (View d : by) if (d.getTag(PIN) == child) d.setTag(PIN, null);
            }
        }

        /** Whether {@code child} is left out of a drawing for {@code viewport} and {@code margin}. */
        private boolean skips(View child, int[] viewport, int margin) {
            if (!far(child, viewport, margin)) return false;
            // The cheap checks first: fields, then the render node's.
            if (child.hasTransientState() || child.getAnimation() != null) return false;
            if (!identity(child)) return false;
            return !pinned(child);
        }

        /** Whether what {@code child} may draw in lies wholly outside {@code viewport} and {@code margin}. */
        private boolean far(View child, int[] viewport, int margin) {
            int left = child.getLeft(), top = child.getTop(), right = child.getRight(), bottom = child.getBottom();
            // Descendants placed outside it (a dropdown under its field).
            android.graphics.RectF o = child instanceof Box ? ((Box) child).overflow : null;
            float[] overflow = null;
            if (o != null) {
                overflow = overflowScratch;
                overflow[0] = o.left;
                overflow[1] = o.top;
                overflow[2] = o.right;
                overflow[3] = o.bottom;
            }
            int[] reach = reachScratch;
            ScrollCull.reach(left, top, right, bottom, overflow, null, reach);
            if (!ScrollCull.outside(reach, viewport[0], viewport[1], viewport[2], viewport[3], margin)) return false;
            // Only a row outside pays for what it draws outside its bounds.
            Decor decor = Mounter.decor(child);
            int shader = child instanceof Box ? ((Box) child).shaderOverflow() : 0;
            boolean paints = decor != null
                    ? ScrollCull.paint(decor.boxShadows, decor.boxShadowColors, decor.outlineWidth, decor.outlineColor,
                            decor.outlineOffset, child.getZ(), shader, paintScratch)
                    : ScrollCull.paint(null, null, 0, 0, 0, child.getZ(), shader, paintScratch);
            if (!paints) return true;
            ScrollCull.reach(left, top, right, bottom, overflow, paintScratch, reach);
            return ScrollCull.outside(reach, viewport[0], viewport[1], viewport[2], viewport[3], margin);
        }

        private static boolean identity(View v) {
            return ScrollCull.identity(v.getTranslationX(), v.getTranslationY(), v.getScaleX(), v.getScaleY(),
                    v.getRotation(), v.getRotationX(), v.getRotationY());
        }

        /** Whether a descendant keeps {@code row} drawn; drops the ones no longer in it. */
        private boolean pinned(View row) {
            if (pins.isEmpty()) return false;
            java.util.ArrayList<View> by = pins.get(row);
            if (by == null) return false;
            for (int i = by.size() - 1; i >= 0; i--) {
                View d = by.get(i);
                // Gone from the row, or its component destroyed.
                boolean still = under(d, row)
                        && (d.getTag(PIN) == row || d instanceof ComponentBox && ((ComponentBox) d).hosted());
                if (!still) {
                    by.remove(i);
                    if (d.getTag(PIN) == row) d.setTag(PIN, null);
                }
            }
            if (!by.isEmpty()) return true;
            pins.remove(row);
            return false;
        }

        /** Whether {@code v} is {@code row} or in it. */
        private static boolean under(View v, View row) {
            for (Object at = v; at instanceof View; at = ((View) at).getParent()) {
                if (at == row) return true;
            }
            return false;
        }

        /** Whether {@code v} keeps its descendants inside its bounds. */
        private static boolean clips(View v) {
            return v instanceof Box && ((Box) v).clip || v instanceof ScrollBox;
        }

        /** Keeps {@code row} drawn for {@code d}; draws again where the last drawing left it out. */
        private void pin(View row, View d) {
            java.util.ArrayList<View> by = pins.get(row);
            if (by == null) pins.put(row, by = new java.util.ArrayList<>(2));
            if (!by.contains(d)) by.add(d);
            if (culled && skipped.contains(row)) {
                culled = false;
                invalidate();
            }
        }

        /** {@code d} no longer pins {@code row} for its transform (an app's component still does). */
        private void unpin(View row, View d) {
            d.setTag(PIN, null);
            if (d instanceof ComponentBox && ((ComponentBox) d).hosted()) return;
            java.util.ArrayList<View> by = pins.get(row);
            if (by == null) return;
            by.remove(d);
            if (by.isEmpty()) pins.remove(row);
        }

        /**
         * How far {@code v}'s transform takes it beyond its own bounds, px
         * (NaN where it cannot be told).
         */
        private float excess(View v) {
            int w = v.getWidth(), h = v.getHeight();
            mapped.set(0, 0, w, h);
            v.getMatrix().mapRect(mapped);
            return ScrollCull.excess(mapped.left, mapped.top, mapped.right, mapped.bottom, w, h);
        }

        /**
         * Pins or unpins {@code row} for descendant {@code v}'s transform;
         * {@code clipped}: an ancestor of it within the row (or the row)
         * clips.
         */
        private void transformed(View v, View row, boolean clipped) {
            // A clipped one never pins: its transform is not even read.
            boolean pin = !clipped && !identity(v)
                    && ScrollCull.pins(excess(v), Math.round(ScrollCull.PIN_EXCESS_DP * mounter.density), clipped);
            Object was = v.getTag(PIN);
            if (pin) {
                if (was == row) return;
                if (was instanceof View) unpinFrom((View) was, v);
                v.setTag(PIN, row);
                pin(row, v);
            } else if (was instanceof View) {
                unpinFrom((View) was, v);
            }
        }

        /** Unpins {@code row}, wherever it went, for {@code d}. */
        private static void unpinFrom(View row, View d) {
            ViewParent p = row.getParent();
            if (p instanceof Content) ((Content) p).unpin(row, d);
            else d.setTag(PIN, null);
        }

        /** Whether the app's component {@code v} holds is in a row: pins it in every content above. */
        private static void hosted(View v) {
            for (View at = v; ; ) {
                ViewParent p = at.getParent();
                if (p instanceof Content) ((Content) p).pin(at, v);
                if (!(p instanceof View)) return;
                at = (View) p;
            }
        }

        /**
         * {@code v} was inserted ({@code Mounter.insert}), or an app's
         * component attached to it: pins the rows above for what it holds
         * that the content cannot bound. A transform on a descendant of a row
         * escapes the row's reach (the core's overflow is the layout's), and
         * {@link #moved} only follows it once it is in the tree; an app's
         * component may draw through a surface (a SurfaceView's map or
         * camera) that stays hidden on some Android versions once its render
         * node leaves the content's display list. A row inserted itself is
         * scanned whole, once.
         */
        static void inserted(View v) {
            View row = v;
            boolean clipped = false;
            ViewParent p = v.getParent();
            while (!(p instanceof Content)) {
                if (!(p instanceof View)) return;
                row = (View) p;
                clipped |= clips(row);
                p = row.getParent();
            }
            ((Content) p).scan(v, row, clipped);
        }

        /** Pins {@code row} for {@code v} and its descendants; {@code clipped}: as {@link #transformed}'s. */
        private void scan(View v, View row, boolean clipped) {
            if (v instanceof ComponentBox && ((ComponentBox) v).hosted()) hosted(v);
            if (v != row && (v.getTag(PIN) != null || !clipped && !identity(v))) transformed(v, row, clipped);
            if (!(v instanceof ViewGroup)) return;
            ViewGroup g = (ViewGroup) v;
            boolean inside = clipped || clips(v);
            for (int i = 0; i < g.getChildCount(); i++) scan(g.getChildAt(i), row, inside);
        }

        /**
         * The viewport moved or changed size (a scroll, the scroll view
         * resized, its padding moved the content): draws the children again
         * once it went half the margin past what they were drawn for.
         * Scrolling replays the display list, which keeps what it left out.
         */
        void viewportChanged() {
            if (!culled) return;
            if (!viewport() || ScrollCull.stale(drawn, drawnMargin, viewport[0], viewport[1], viewport[2], viewport[3])) {
                culled = false;
                invalidate();
            }
        }

        /**
         * {@code v} may have moved, changed size, transform, decoration or
         * reach. A view property (a transform from styles, transitions,
         * keyframes or animated values, a sticky cell pinned) updates only
         * its render node, and a new overflow area or decoration of a row
         * changes only what this content decides: where {@code v} is a row
         * this content left out and would draw now, it draws again. A frame
         * change ({@code Box.place}) already has the parent record again
         * (`View.setFrame` invalidates it) and calls this as a guard. A row it
         * drew is in its display list wherever it goes; one still left out
         * (an animation of its opacity, a row moved further away) costs
         * nothing. Below a row, a transform pins or unpins the row
         * ({@link #inserted}); a view in no content, or with no transform
         * and none pinning, returns at once.
         */
        static void moved(View v) {
            ViewParent p = v.getParent();
            if (p instanceof Content) {
                Content content = (Content) p;
                if (content.culled && content.skipped.contains(v) && !content.skips(v, content.drawn, content.drawnMargin)) {
                    content.culled = false;
                    content.invalidate();
                }
                return;
            }
            if (!(p instanceof View) || v.getTag(PIN) == null && identity(v)) return;
            View row = v;
            boolean clipped = false;
            while (!(p instanceof Content)) {
                if (!(p instanceof View)) {
                    // Out of the tree: no longer pins where it was.
                    Object was = v.getTag(PIN);
                    if (was instanceof View) unpinFrom((View) was, v);
                    return;
                }
                row = (View) p;
                clipped |= clips(row);
                p = row.getParent();
            }
            ((Content) p).transformed(v, row, clipped);
        }

        @Override
        protected void onMeasure(int widthSpec, int heightSpec) {
            int w = Math.max(contentWidth, MeasureSpec.getSize(widthSpec));
            int h = Math.max(contentHeight, MeasureSpec.getSize(heightSpec));
            super.onMeasure(MeasureSpec.makeMeasureSpec(w, MeasureSpec.EXACTLY), MeasureSpec.makeMeasureSpec(h, MeasureSpec.EXACTLY));
        }
    }

    ScrollBox(Context context, Mounter mounter, int id) {
        super(context);
        this.mounter = mounter;
        this.id = id;
        setBackground(decor);
        content = new Content(context, mounter, id);
        spinner = new ProgressBar(context);
        spinner.setIndeterminate(true);
        spinner.setVisibility(GONE);
        int size = Math.round(28 * mounter.density);
        addView(spinner, new LayoutParams(size, size, android.view.Gravity.CENTER_HORIZONTAL | android.view.Gravity.TOP));
        content.addOnLayoutChangeListener((v, l, t, r, b, ol, ot, or, ob) -> {
            // Padding (`contentInset`, the keyboard's room) moved it.
            content.viewportChanged();
            payOwed();
        });
        build();
    }

    @Override
    public int nodeId() {
        return id;
    }

    private void build() {
        if (scroller != null) {
            scroller.removeView(content);
            removeView(scroller);
            if (scroller instanceof ZScroll) {
                content.setScaleX(1);
                content.setScaleY(1);
                content.setTranslationX(0);
                content.setTranslationY(0);
                mounter.zoom(id, 1);
            }
        }
        scroller = zooms ? new ZScroll(getContext()) : horizontal ? new HScroll(getContext()) : new VScroll(getContext());
        scroller.setOverScrollMode(bounces ? OVER_SCROLL_IF_CONTENT_SCROLLS : OVER_SCROLL_NEVER);
        scroller.setPadding(insetLeft, insetTop, insetRight, insetBottom + keyboardPadding);
        scroller.setClipToPadding(false);
        // A scroll view inside another one scrolls first, then hands the
        // rest to its parent (else the parent takes every drag along its axis).
        scroller.setNestedScrollingEnabled(true);
        scroller.setVerticalScrollBarEnabled(!horizontal);
        scroller.setHorizontalScrollBarEnabled(horizontal);
        scroller.addView(content, new FrameLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT));
        // The view or its content changed size: scroll what was clamped,
        // and draw what a larger viewport shows.
        scroller.addOnLayoutChangeListener((v, l, t, r, b, ol, ot, or, ob) -> {
            content.viewportChanged();
            payOwed();
        });
        scroller.setOnScrollChangeListener((v, x, y, ox, oy) -> {
            // Every scroll comes here (drags, flings, smooth and
            // programmatic scrolls): the content draws what comes near.
            content.viewportChanged();
            // Layout may clamp against a mixture of old and new pixel
            // sizes. Report only after restoring the logical offset.
            if (rescaling) return;
            if (stickyCells.length > 0) pinSticky();
            if (dispatching && touching && !dragging) {
                dragging = true;
                // The scroll view takes the touches from JS responders.
                mounter.cancelTouches();
                // Caught while gliding: the glide ended.
                if (momentum) emit(Native.MOMENTUM_END);
                momentum = false;
                emit(Native.SCROLL_DRAG_START);
            }
            float d = mounter.density;
            mounter.scroll(id, x / d, y / d, applied);
            removeCallbacks(settled);
            postDelayed(settled, 120);
            if (x != ox || y != oy) for (NshScrollListener l : hostListeners) l.onScroll(x, y, x - ox, y - oy);
        });
        addView(scroller, 0, new LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT));
    }

    void configure(boolean horizontal, boolean indicator, int dismissMode) {
        if (horizontal != this.horizontal) {
            this.horizontal = horizontal;
            build();
        }
        scroller.setVerticalScrollBarEnabled(indicator && !horizontal);
        scroller.setHorizontalScrollBarEnabled(indicator && horizontal);
        this.dismissMode = dismissMode;
    }

    void setContentSize(int width, int height) {
        if (rescaling) {
            rescaledContent = true;
            requestLayout();
        }
        int oldWidth = content.contentWidth, oldHeight = content.contentHeight;
        content.contentWidth = width;
        content.contentHeight = height;
        if (scroller instanceof ZScroll) {
            // Exactly the core's size: the zoom scales it.
            ((ZScroll) scroller).placeContent();
        } else {
            // Sized right away: scrolling in the same batch (anchoring, a
            // jump to an item) is clamped to the content's current size.
            int w = Math.max(width, scroller.getWidth()), h = Math.max(height, scroller.getHeight());
            content.measure(MeasureSpec.makeMeasureSpec(w, MeasureSpec.EXACTLY), MeasureSpec.makeMeasureSpec(h, MeasureSpec.EXACTLY));
            content.layout(0, 0, w, h);
            content.requestLayout();
        }
        if (width != oldWidth || height != oldHeight) {
            for (NshScrollListener l : hostListeners) l.onContentSizeChange(hostContentWidth(), hostContentHeight());
        }
    }

    /** Sticky cells: node ids and their travel (px), from `StickyCells`. */
    private int[] stickyCells = new int[0];
    private float[] stickyStart = new float[0], stickyEnd = new float[0];

    void setSticky(int[] cells, float[] start, float[] end) {
        for (int old : stickyCells) {
            boolean kept = false;
            for (int c : cells) kept |= c == old;
            View v = mounter.view(old);
            if (!kept && v != null) {
                if (horizontal) v.setTranslationX(0); else v.setTranslationY(0);
            }
            if (!kept && mounter.stickyHosts.get(old) == this) mounter.stickyHosts.remove(old);
            mounter.forgetStickyOffset(old);
        }
        // A cell may be in a list in this scroll view's viewport: its
        // animated offset finds this scroll view. Its offset is forgotten:
        // the outputs after the batch bring back the ones still animated,
        // so a cell whose offset became a number no longer adds the old one.
        for (int c : cells) {
            mounter.stickyHosts.put(c, this);
            mounter.forgetStickyOffset(c);
        }
        stickyCells = cells;
        stickyStart = start;
        stickyEnd = end;
        frontSticky();
        pinSticky();
    }

    /** Keeps sticky cells above the others (also after new cells arrive). */
    void frontSticky() {
        for (int c : stickyCells) {
            View v = mounter.view(c);
            if (v != null && v.getParent() == content && content.indexOfChild(v) != content.getChildCount() - 1) {
                v.bringToFront();
            }
        }
    }

    boolean hasSticky() {
        return stickyCells.length > 0;
    }

    /** Moves the sticky cells with the scroll offset (and their animated offsets). */
    void pinSticky() {
        float offset = horizontal ? scroller.getScrollX() : scroller.getScrollY();
        for (int i = 0; i < stickyCells.length; i++) {
            View v = mounter.view(stickyCells[i]);
            if (v == null) continue;
            float extra = mounter.stickyOffset(stickyCells[i]);
            float t = Math.max(0, Math.min(offset + extra - stickyStart[i], stickyEnd[i] - stickyStart[i]));
            if (horizontal) v.setTranslationX(t); else v.setTranslationY(t);
            Content.moved(v);
        }
    }

    /** Room below the content (keyboard avoidance), under `contentInset`. */
    void setContentPaddingBottom(int px) {
        keyboardPadding = px;
        scroller.setPadding(insetLeft, insetTop, insetRight, insetBottom + keyboardPadding);
    }

    /** `ScrollBehavior` (lengths in dp). */
    void setBehavior(boolean paging, float snapInterval, float[] snapOffsets, int snapAlignment, float deceleration,
            boolean enabled, boolean bounces, float top, float right, float bottom, float left, int keyboardTaps,
            boolean oneStep) {
        this.paging = paging;
        this.snapInterval = snapInterval;
        this.snapOffsets = snapOffsets;
        this.snapAlignment = snapAlignment;
        this.deceleration = deceleration;
        this.enabled = enabled;
        this.bounces = bounces;
        this.keyboardTaps = keyboardTaps;
        this.oneStep = oneStep;
        float d = mounter.density;
        insetTop = Math.round(top * d);
        insetRight = Math.round(right * d);
        insetBottom = Math.round(bottom * d);
        insetLeft = Math.round(left * d);
        scroller.setOverScrollMode(bounces ? OVER_SCROLL_IF_CONTENT_SCROLLS : OVER_SCROLL_NEVER);
        scroller.setPadding(insetLeft, insetTop, insetRight, insetBottom + keyboardPadding);
    }

    // --- Paging and snapping ---------------------------------------------

    private boolean snaps() {
        return paging || snapInterval > 0 || snapOffsets.length > 0;
    }

    /** The scroll view's own fling (px/s): snaps instead when snapping. */
    private boolean fling(int velocity) {
        flung = true;
        if (!snaps()) return false;
        snap(velocity);
        return true;
    }

    /** Whether this view took the DOWN of the touch that went down at {@code downTime}. */
    boolean took(long downTime) {
        return this.downTime == downTime;
    }

    /** Whether a drag by ({@code dx}, {@code dy}) px along this view's axis has content to move. */
    boolean scrolls(float dx, float dy) {
        if (scroller == null || !enabled) return false;
        if (horizontal) return Math.abs(dx) > Math.abs(dy) && scroller.canScrollHorizontally(dx < 0 ? 1 : -1);
        return Math.abs(dy) > Math.abs(dx) && scroller.canScrollVertically(dy < 0 ? 1 : -1);
    }

    /** Glides to the snap point for a release at {@code velocity} (px/s). */
    private void snap(int velocity) {
        if (scroller instanceof ZScroll) return;
        float d = mounter.density;
        boolean h = horizontal;
        float offset = (h ? scroller.getScrollX() : scroller.getScrollY()) / d;
        float viewport = (h ? scroller.getWidth() : scroller.getHeight()) / d;
        float length = (h ? content.getWidth() + scroller.getPaddingLeft() + scroller.getPaddingRight()
                : content.getHeight() + scroller.getPaddingTop() + scroller.getPaddingBottom()) / d;
        float v = velocity / d / 1000f;
        // Where UIKit's deceleration would glide (velocity kept per ms).
        float rate = deceleration > 0 ? deceleration : (snapInterval > 0 || snapOffsets.length > 0) ? 0.99f : 0.998f;
        float projected = offset + v * rate / (1 - rate);
        float target = snapTarget(offset, projected, v, viewport, length);
        int px = Math.round(target * d);
        if (scroller instanceof ScrollView) ((ScrollView) scroller).smoothScrollTo(0, px);
        else ((HorizontalScrollView) scroller).smoothScrollTo(px, 0);
    }

    /** `ScrollBehavior::snap_target` (nsh-core), in dp. */
    private float snapTarget(float offset, float projected, float velocity, float viewport, float content) {
        float max = Math.max(0, content - viewport);
        float[] points;
        if (snapInterval > 0) {
            float shift = snapAlignment == 1 ? (viewport - snapInterval) / 2 : snapAlignment == 2 ? viewport - snapInterval : 0;
            int n = (int) Math.max(0, Math.ceil((max + shift) / snapInterval));
            points = new float[n + 1];
            for (int i = 0; i <= n; i++) points[i] = Math.max(0, Math.min(max, i * snapInterval - shift));
        } else if (snapOffsets.length > 0) {
            points = new float[snapOffsets.length];
            for (int i = 0; i < points.length; i++) points[i] = Math.max(0, Math.min(max, snapOffsets[i]));
        } else {
            float page = Math.max(1, viewport);
            int n = (int) Math.ceil(max / page);
            points = new float[n + 1];
            for (int i = 0; i <= n; i++) points[i] = Math.min(max, i * page);
        }
        final float slow = 0.3f; // dp per ms: slower releases settle at the nearest point
        boolean step = oneStep || (paging && snapInterval <= 0 && snapOffsets.length == 0);
        float from = step ? offset : projected;
        Float target = null;
        if (velocity > slow) {
            for (float p : points) {
                if (step ? p > from + 0.5f : p >= from - 0.5f) { target = p; break; }
            }
        } else if (velocity < -slow) {
            for (int i = points.length - 1; i >= 0; i--) {
                float p = points[i];
                if (step ? p < from - 0.5f : p <= from + 0.5f) { target = p; break; }
            }
        } else {
            for (float p : points) {
                if (target == null || Math.abs(p - from) < Math.abs(target - from)) target = p;
            }
        }
        return target != null ? target : velocity > 0 ? max : 0;
    }

    /** The platform scroll views, with `scrollable` and snapping. */
    private final class VScroll extends ScrollView {
        VScroll(Context context) {
            super(context);
        }

        @Override
        public boolean dispatchTouchEvent(MotionEvent e) {
            if (!claim(this, e)) return super.dispatchTouchEvent(e);
            // Handed over: the platform scroll view would start its own drag on this move and ask the
            // views around not to intercept again, so it ends its touch here.
            MotionEvent cancel = MotionEvent.obtain(e);
            cancel.setAction(MotionEvent.ACTION_CANCEL);
            super.dispatchTouchEvent(cancel);
            cancel.recycle();
            return false;
        }

        @Override
        public boolean onInterceptTouchEvent(MotionEvent e) {
            return enabled && !jsBlocked && !gateBlocked && super.onInterceptTouchEvent(e);
        }

        @SuppressLint("ClickableViewAccessibility")
        @Override
        public boolean onTouchEvent(MotionEvent e) {
            if (!enabled || jsBlocked || gateBlocked) return false;
            boolean up = beforeTouch(e);
            boolean handled = super.onTouchEvent(e);
            if (up) released();
            return handled;
        }

        @Override
        public void fling(int velocity) {
            if (!ScrollBox.this.fling(velocity)) super.fling(velocity);
        }

        void stop() {
            // Bypass snapping: a zero-velocity native fling replaces the
            // previous scroller animation at its current position.
            super.fling(0);
        }

        @Override
        public void draw(Canvas canvas) {
            super.draw(canvas);
            frames++;
        }
    }

    private final class HScroll extends HorizontalScrollView {
        HScroll(Context context) {
            super(context);
        }

        @Override
        public boolean dispatchTouchEvent(MotionEvent e) {
            if (!claim(this, e)) return super.dispatchTouchEvent(e);
            // Handed over: the platform scroll view would start its own drag on this move and ask the
            // views around not to intercept again, so it ends its touch here.
            MotionEvent cancel = MotionEvent.obtain(e);
            cancel.setAction(MotionEvent.ACTION_CANCEL);
            super.dispatchTouchEvent(cancel);
            cancel.recycle();
            return false;
        }

        @Override
        public boolean onInterceptTouchEvent(MotionEvent e) {
            return enabled && !jsBlocked && !gateBlocked && super.onInterceptTouchEvent(e);
        }

        @SuppressLint("ClickableViewAccessibility")
        @Override
        public boolean onTouchEvent(MotionEvent e) {
            if (!enabled || jsBlocked || gateBlocked) return false;
            boolean up = beforeTouch(e);
            boolean handled = super.onTouchEvent(e);
            if (up) released();
            return handled;
        }

        @Override
        public void fling(int velocity) {
            if (!ScrollBox.this.fling(velocity)) super.fling(velocity);
        }

        void stop() {
            // Bypass snapping: a zero-velocity native fling replaces the
            // previous scroller animation at its current position.
            super.fling(0);
        }

        @Override
        public void draw(Canvas canvas) {
            super.draw(canvas);
            frames++;
        }
    }

    /** Where the touch that {@link #claim} is deciding went down; deciding while true. */
    private float claimX, claimY;
    private boolean claiming, yielded, owning;
    /** Owning the drag: where the last move was, to tell when it reaches an edge. */
    private float lastX, lastY;

    /**
     * A scroll view around this one along the same axis (a pager) intercepts the move that passes the
     * touch slop before this view sees it. From the down, while this view has content to move, it asks
     * the views around not to intercept; the first move past the slop decides: along the axis towards
     * content this view keeps the drag, otherwise the views around get the next move (as ZScroll does).
     * Returns whether this view gave the touch away: it no longer sees the touch.
     */
    private boolean claim(View scroller, MotionEvent e) {
        ViewParent parent = scroller.getParent();
        if (parent == null) return false;
        if (yielded) {
            int a = e.getActionMasked();
            if (a == MotionEvent.ACTION_DOWN) yielded = false;
            else {
                if (a == MotionEvent.ACTION_UP || a == MotionEvent.ACTION_CANCEL) yielded = false;
                return true;
            }
        }
        switch (e.getActionMasked()) {
            case MotionEvent.ACTION_DOWN:
                claimX = e.getX();
                claimY = e.getY();
                owning = false;
                claiming = enabled && !jsBlocked && !gateBlocked && (horizontal
                        ? scroller.canScrollHorizontally(1) || scroller.canScrollHorizontally(-1)
                        : scroller.canScrollVertically(1) || scroller.canScrollVertically(-1));
                if (claiming) parent.requestDisallowInterceptTouchEvent(true);
                break;
            case MotionEvent.ACTION_MOVE:
                if (owning) {
                    // Holding the drag: at the edge it moves towards, the rest of the drag goes to
                    // the views around (a pager's next page), as ZScroll does.
                    float mx = e.getX() - lastX, my = e.getY() - lastY;
                    lastX = e.getX();
                    lastY = e.getY();
                    float along = horizontal ? mx : my;
                    boolean more = horizontal ? scroller.canScrollHorizontally(along < 0 ? 1 : -1)
                            : scroller.canScrollVertically(along < 0 ? 1 : -1);
                    if (along != 0 && !more) {
                        owning = false;
                        parent.requestDisallowInterceptTouchEvent(false);
                        yielded = true;
                        return true;
                    }
                    break;
                }
                if (!claiming) break;
                float dx = e.getX() - claimX, dy = e.getY() - claimY;
                int slop = ViewConfiguration.get(scroller.getContext()).getScaledTouchSlop();
                if (Math.abs(dx) <= slop && Math.abs(dy) <= slop) break;
                claiming = false;
                if (!scrolls(dx, dy)) {
                    parent.requestDisallowInterceptTouchEvent(false);
                    yielded = true;
                    return true;
                }
                owning = true;
                lastX = e.getX();
                lastY = e.getY();
                break;
            case MotionEvent.ACTION_UP:
            case MotionEvent.ACTION_CANCEL:
                claiming = false;
                owning = false;
                break;
            default:
                break;
        }
        return false;
    }

    /** Before the scroll view sees {@code e}: whether it ends the touch. */
    private boolean beforeTouch(MotionEvent e) {
        int action = e.getActionMasked();
        boolean up = action == MotionEvent.ACTION_UP || action == MotionEvent.ACTION_CANCEL;
        if (up) flung = false;
        return up;
    }

    /** The finger lifted off the scroll view (after its fling, if any). */
    private void released() {
        if (!dragging) return;
        if (!flung && snaps()) {
            snap(0);
            flung = true;
        }
        emit(Native.SCROLL_DRAG_END);
        if (flung) {
            momentum = true;
            emit(Native.MOMENTUM_START);
        }
    }

    private void emit(int kind) {
        if (!mounter.wants(id, kind)) return;
        float d = mounter.density;
        mounter.event(id, kind, null, 0, scroller.getScrollX() / d, scroller.getScrollY() / d);
    }

    /**
     * Scrolls a descendant into the area above the bottom padding (the
     * keyboard). `requestRectangleOnScreen` cannot: ScrollView ignores its
     * padding when it decides whether a rectangle is visible.
     */
    /**
     * Scrolls {@code v} (and {@code margin} px around it) into view, over
     * {@code motion}: an animator from 0 to 1 the caller starts. False when
     * there is nothing to scroll (the animator is left alone).
     */
    boolean reveal(View v, int margin, ValueAnimator motion) {
        Rect r = new Rect(0, -margin, v.getWidth(), v.getHeight() + margin);
        scroller.offsetDescendantRectToMyCoords(v, r);
        int top = scroller.getScrollY();
        int bottom = top + scroller.getHeight() - scroller.getPaddingBottom();
        int dy = r.bottom > bottom ? Math.min(r.bottom - bottom, r.top - top) : Math.min(0, r.top - top);
        if (dy == 0) return false;
        if (!(scroller instanceof ScrollView)) {
            scroller.scrollBy(0, dy);
            return false;
        }
        int x = scroller.getScrollX();
        motion.addUpdateListener(a -> scroller.scrollTo(x, top + Math.round(dy * (float) a.getAnimatedValue())));
        return true;
    }

    /** Saved pixel target in the new density, until content and viewport are laid out. */
    private boolean rescaling, rescaledContent;
    private int rescaledX, rescaledY;

    void rescale(float scale) {
        rescaledX = Math.round((rescaling ? rescaledX : scroller.getScrollX() + owedX) * scale);
        rescaledY = Math.round((rescaling ? rescaledY : scroller.getScrollY() + owedY) * scale);
        rescaling = true;
        rescaledContent = false;
        owedX = owedY = 0;
        removeCallbacks(settled);
        // Native motion also uses pixels: end it before restoring the
        // position, so an old fling cannot move it back on the next frame.
        stopMotion();
    }

    private void stopMotion() {
        if (scroller instanceof ZScroll) ((ZScroll) scroller).stop();
        else if (scroller instanceof VScroll) ((VScroll) scroller).stop();
        else if (scroller instanceof HScroll) ((HScroll) scroller).stop();
    }

    @Override
    protected void onLayout(boolean changed, int l, int t, int r, int b) {
        super.onLayout(changed, l, t, r, b);
        if (!rescaling || !rescaledContent) return;
        // Unlike a core scroll command, changing coordinate systems does
        // not increment `applied`. A smaller content area may clamp this
        // final offset; tell the core so virtual lists follow the result.
        if (scroller instanceof ZScroll) ((ZScroll) scroller).scrollToPx(rescaledX, rescaledY, false);
        else scroller.scrollTo(rescaledX, rescaledY);
        // The zero-velocity native scroller must also start at this new
        // position, rather than writing its pre-rescale position next frame.
        stopMotion();
        rescaling = false;
        pinSticky();
        float d = mounter.density;
        mounter.scroll(id, scroller.getScrollX() / d, scroller.getScrollY() / d, applied);
        postDelayed(settled, 120);
    }

    /** Scrolls the core asked for that were applied (reported with offsets). */
    private int applied;

    /**
     * The part of the last {@code scrollByPx} or {@code scrollToPx} that the
     * scroll view clamped (px): the view or its content is about to change
     * size (e.g. while a layout animation resizes it, or a pager's pages
     * arrive after the command) and catches up when it does, until the user
     * scrolls.
     */
    private int owedX, owedY;

    void scrollByPx(int dx, int dy) {
        applied++;
        if (rescaling) {
            rescaledX += dx;
            rescaledY += dy;
            return;
        }
        int x = scroller.getScrollX() + dx + owedX, y = scroller.getScrollY() + dy + owedY;
        scroller.scrollTo(x, y);
        owedX = x - scroller.getScrollX();
        owedY = y - scroller.getScrollY();
    }

    /** Pays what clamping kept back, as far as the current size allows. */
    private void payOwed() {
        if (rescaling || (owedX == 0 && owedY == 0)) return;
        int x = scroller.getScrollX() + owedX, y = scroller.getScrollY() + owedY;
        scroller.scrollTo(x, y);
        owedX = x - scroller.getScrollX();
        owedY = y - scroller.getScrollY();
    }

    void scrollToPx(int x, int y, boolean animated) {
        applied++;
        if (rescaling) {
            rescaledX = x;
            rescaledY = y;
            return;
        }
        if (scroller instanceof ZScroll) {
            owedX = owedY = 0;
            ((ZScroll) scroller).scrollToPx(x, y, animated);
            return;
        }
        // Where the scroll view can go now; the rest is owed until the
        // content (or the view) grows to it (`payOwed`).
        int maxX = Math.max(0, content.getWidth() - scroller.getWidth());
        int maxY = Math.max(0, content.getHeight() - scroller.getHeight());
        int toX = Math.max(0, Math.min(x, maxX)), toY = Math.max(0, Math.min(y, maxY));
        owedX = x - toX;
        owedY = y - toY;
        if (scroller instanceof ScrollView) {
            if (animated) ((ScrollView) scroller).smoothScrollTo(toX, toY);
            else scroller.scrollTo(toX, toY);
        } else {
            if (animated) ((HorizontalScrollView) scroller).smoothScrollTo(toX, toY);
            else scroller.scrollTo(toX, toY);
        }
    }

    // --- The host's handle (`NshScrollHandle`) ---------------------------

    int hostScrollX() {
        return scroller.getScrollX();
    }

    int hostScrollY() {
        return scroller.getScrollY();
    }

    int hostContentWidth() {
        return Math.max(content.contentWidth, scroller.getWidth());
    }

    int hostContentHeight() {
        return Math.max(content.contentHeight, scroller.getHeight());
    }

    int hostViewportWidth() {
        return scroller.getWidth();
    }

    int hostViewportHeight() {
        return scroller.getHeight();
    }

    /** Scrolls as the user would: not one of the core's scrolls (`applied`). */
    void hostScrollBy(int dx, int dy, boolean animated) {
        if (scroller instanceof ZScroll) {
            ((ZScroll) scroller).scrollToPx(scroller.getScrollX() + dx, scroller.getScrollY() + dy, animated);
        } else if (animated && scroller instanceof ScrollView) {
            hostAnimating = true;
            ((ScrollView) scroller).smoothScrollBy(dx, dy);
        } else if (animated && scroller instanceof HorizontalScrollView) {
            hostAnimating = true;
            ((HorizontalScrollView) scroller).smoothScrollBy(dx, dy);
        } else {
            scroller.scrollBy(dx, dy);
        }
    }

    /** Ends a fling or a smooth scroll where it is; nothing when at rest. */
    void hostStop() {
        if (!momentum && !hostAnimating) return;
        hostAnimating = false;
        if (scroller instanceof ZScroll) {
            ((ZScroll) scroller).stop();
        } else {
            // The first empty smooth scroll replaces the animation with one
            // that still runs (a scroll view that sees a touch down then
            // takes the whole gesture, a horizontal scroll view inside never
            // gets it); the second, within the platform's gap between
            // animated scrolls, aborts it. A zero fling (`stopMotion`) would
            // leave the running one.
            for (int i = 0; i < 2; i++) {
                if (scroller instanceof ScrollView) ((ScrollView) scroller).smoothScrollBy(0, 0);
                else if (scroller instanceof HorizontalScrollView) ((HorizontalScrollView) scroller).smoothScrollBy(0, 0);
            }
        }
        if (momentum) {
            momentum = false;
            emit(Native.MOMENTUM_END);
        }
    }

    /** Scrolling came to rest (no API for it: no change for a moment). */
    private void settled() {
        hostAnimating = false;
        if (touching) {
            postDelayed(settled, 120);
            return;
        }
        if (momentum) {
            momentum = false;
            emit(Native.MOMENTUM_END);
        }
        mounter.scrollEnded(id);
        float d = mounter.density;
        mounter.event(id, Native.SCROLL_END, null, 0, scroller.getScrollX() / d, scroller.getScrollY() / d);
    }

    // --- Zoom ------------------------------------------------------------

    /** `ScrollBehavior` zoom settings. */
    private boolean zooms, bouncesZoom = true, pinch = true, centerContent;
    private float minZoom = 1, maxZoom = 1;

    /** Pull to refresh: whether the spinner shows, where (dp) a pull refreshes and the room kept while refreshing. */
    private boolean refreshIndicator = true;
    private float refreshThreshold = 64, refreshInset = 56;

    /** `refreshIndicator`: the app draws its own, {@code height} dp tall, in place of the spinner. */
    void setRefresh(boolean own, float height) {
        float inset = own ? height : 56;
        boolean changed = refreshIndicator == own || inset != refreshInset;
        refreshIndicator = !own;
        refreshThreshold = own ? height : 64;
        refreshInset = inset;
        if (!changed) return;
        if (own) {
            spinner.setVisibility(GONE);
        } else if (refreshing) {
            spinner.setVisibility(VISIBLE);
            spinner.setAlpha(1);
        }
        // A refresh in progress keeps the room of the indicator now showing.
        if (refreshing && !pulling) animatePull(inset * mounter.density, null);
    }

    void setZoom(boolean zooms, float min, float max, boolean bouncesZoom, boolean pinch, boolean center) {
        minZoom = min;
        maxZoom = max;
        this.bouncesZoom = bouncesZoom;
        this.pinch = pinch;
        centerContent = center;
        if (zooms != this.zooms) {
            this.zooms = zooms;
            build();
        } else if (scroller instanceof ZScroll) {
            ZScroll z = (ZScroll) scroller;
            z.setScale(Math.max(minZoom, Math.min(maxZoom, z.scale)), z.getWidth() / 2f, z.getHeight() / 2f);
        }
    }

    /** `zoomTo` (content point in dp). */
    void zoomTo(float scale, boolean hasCenter, float x, float y, boolean animated) {
        if (!(scroller instanceof ZScroll)) return;
        float d = mounter.density;
        ((ZScroll) scroller).zoomTo(scale, hasCenter, x * d, y * d, animated);
    }

    /**
     * A zoomable scroll view (UIScrollView's zooming, which Android lacks):
     * the content is scaled around its top left corner and scrolled both
     * ways by this view's own scroll offset. Pinches zoom around their focal
     * point, drags (any number of fingers) pan and fling, and both bounce
     * past the limits when allowed.
     */
    private final class ZScroll extends FrameLayout {
        float scale = 1;
        /** The pinch's scale without the rubber band beyond the limits. */
        private float rawScale = 1;
        private final ScaleGestureDetector detector;
        private final OverScroller glide;
        private VelocityTracker velocity;
        private final int slop, minFling, maxFling;
        private float downX, downY, lastX, lastY;
        private boolean panning;
        /** The touch went down on content that pans: the first drag past the slop is this view's to give away. */
        private boolean deciding;
        /** Panning at an edge, the drag outward is left to a scroll view around (a pager's next page). */
        private boolean yielding;
        private android.animation.ValueAnimator zoomAnimation;

        ZScroll(Context context) {
            super(context);
            ViewConfiguration c = ViewConfiguration.get(context);
            slop = c.getScaledTouchSlop();
            minFling = c.getScaledMinimumFlingVelocity();
            maxFling = c.getScaledMaximumFlingVelocity();
            glide = new OverScroller(context);
            detector = new ScaleGestureDetector(context, new ScaleGestureDetector.SimpleOnScaleGestureListener() {
                @Override
                public boolean onScaleBegin(ScaleGestureDetector d) {
                    if (!pinch) return false;
                    stop();
                    rawScale = scale;
                    startPan();
                    return true;
                }

                @Override
                public boolean onScale(ScaleGestureDetector d) {
                    rawScale *= d.getScaleFactor();
                    setScale(rubber(rawScale), d.getFocusX(), d.getFocusY());
                    return true;
                }
            });
            detector.setQuickScaleEnabled(false);
        }

        /** Beyond the limits a pinch meets growing resistance (or none goes there). */
        private float rubber(float s) {
            if (s > maxZoom) return bouncesZoom ? maxZoom * (float) Math.pow(s / maxZoom, 0.35) : maxZoom;
            if (s < minZoom) return bouncesZoom ? minZoom * (float) Math.pow(s / minZoom, 0.35) : minZoom;
            return s;
        }

        private float contentWidth() {
            return content.contentWidth * scale;
        }

        private float contentHeight() {
            return content.contentHeight * scale;
        }

        /** `centerContent`: where content narrower than the viewport starts. */
        private float offsetX(float s) {
            float free = getWidth() - getPaddingLeft() - getPaddingRight() - content.contentWidth * s;
            return centerContent ? Math.max(0, free / 2) : 0;
        }

        private float offsetY(float s) {
            float free = getHeight() - getPaddingTop() - getPaddingBottom() - content.contentHeight * s;
            return centerContent ? Math.max(0, free / 2) : 0;
        }

        private int maxX() {
            return (int) Math.max(0, getPaddingLeft() + contentWidth() + getPaddingRight() - getWidth());
        }

        private int maxY() {
            return (int) Math.max(0, getPaddingTop() + contentHeight() + getPaddingBottom() - getHeight());
        }

        @Override
        protected void onMeasure(int widthSpec, int heightSpec) {
            setMeasuredDimension(MeasureSpec.getSize(widthSpec), MeasureSpec.getSize(heightSpec));
            measureContent();
        }

        private void measureContent() {
            content.measure(MeasureSpec.makeMeasureSpec(content.contentWidth, MeasureSpec.EXACTLY),
                    MeasureSpec.makeMeasureSpec(content.contentHeight, MeasureSpec.EXACTLY));
        }

        @Override
        protected void onLayout(boolean changed, int l, int t, int r, int b) {
            layoutContent();
        }

        private void layoutContent() {
            int x = getPaddingLeft(), y = getPaddingTop();
            content.layout(x, y, x + content.contentWidth, y + content.contentHeight);
            apply();
        }

        /** The core's content size changed. */
        void placeContent() {
            measureContent();
            layoutContent();
            scrollTo(clamp(getScrollX(), maxX()), clamp(getScrollY(), maxY()));
        }

        private void apply() {
            // Around the top left (a Box resets its pivot when resized).
            content.setPivotX(0);
            content.setPivotY(0);
            content.setScaleX(scale);
            content.setScaleY(scale);
            content.setTranslationX(offsetX(scale));
            content.setTranslationY(offsetY(scale));
        }

        /** Zooms to {@code s} keeping the content under ({@code fx}, {@code fy}) (view px) in place. */
        void setScale(float s, float fx, float fy) {
            float px = (getScrollX() + fx - getPaddingLeft() - offsetX(scale)) / scale;
            float py = (getScrollY() + fy - getPaddingTop() - offsetY(scale)) / scale;
            place(s, px, py, fx, fy);
        }

        /** Zooms to {@code s} with content point ({@code px}, {@code py}) (unzoomed px) at ({@code fx}, {@code fy}). */
        private void place(float s, float px, float py, float fx, float fy) {
            boolean changed = s != scale;
            scale = s;
            apply();
            int x = Math.round(px * s + getPaddingLeft() + offsetX(s) - fx);
            int y = Math.round(py * s + getPaddingTop() + offsetY(s) - fy);
            int sx = getScrollX(), sy = getScrollY();
            scrollTo(clamp(x, maxX()), clamp(y, maxY()));
            if (changed) {
                mounter.zoom(id, s);
                // The new scale goes out with an offset report.
                if (sx == getScrollX() && sy == getScrollY()) {
                    float d = mounter.density;
                    mounter.scroll(id, getScrollX() / d, getScrollY() / d, applied);
                }
            }
        }

        private int clamp(int v, int max) {
            return Math.max(0, Math.min(max, v));
        }

        void zoomTo(float target, boolean hasCenter, float x, float y, boolean animated) {
            stop();
            float to = Math.max(minZoom, Math.min(maxZoom, target));
            float w = getWidth() / 2f, h = getHeight() / 2f;
            float fromX = (getScrollX() + w - getPaddingLeft() - offsetX(scale)) / scale;
            float fromY = (getScrollY() + h - getPaddingTop() - offsetY(scale)) / scale;
            float toX = hasCenter ? x : fromX, toY = hasCenter ? y : fromY;
            if (!animated) {
                place(to, toX, toY, w, h);
                return;
            }
            float from = scale;
            zoomAnimation = android.animation.ValueAnimator.ofFloat(0, 1);
            zoomAnimation.setDuration(300);
            zoomAnimation.setInterpolator(new android.view.animation.DecelerateInterpolator(1.5f));
            zoomAnimation.addUpdateListener(a -> {
                float t = (float) a.getAnimatedValue();
                place(from + (to - from) * t, fromX + (toX - fromX) * t, fromY + (toY - fromY) * t, w, h);
            });
            zoomAnimation.start();
        }

        void scrollToPx(int x, int y, boolean animated) {
            stop();
            int tx = clamp(x, maxX()), ty = clamp(y, maxY());
            if (animated) {
                glide.startScroll(getScrollX(), getScrollY(), tx - getScrollX(), ty - getScrollY(), 300);
                postInvalidateOnAnimation();
            } else {
                scrollTo(tx, ty);
            }
        }

        private void stop() {
            if (!glide.isFinished()) glide.forceFinished(true);
            if (zoomAnimation != null) {
                zoomAnimation.cancel();
                zoomAnimation = null;
            }
        }

        /** The focal point of the fingers down (without one lifting now). */
        private float focus(MotionEvent e, boolean x) {
            int skip = e.getActionMasked() == MotionEvent.ACTION_POINTER_UP ? e.getActionIndex() : -1;
            float sum = 0;
            int n = 0;
            for (int i = 0; i < e.getPointerCount(); i++) {
                if (i == skip) continue;
                sum += x ? e.getX(i) : e.getY(i);
                n++;
            }
            return n > 0 ? sum / n : 0;
        }

        private void startPan() {
            panning = true;
            getParent().requestDisallowInterceptTouchEvent(true);
        }

        /** Follows the fingers; true once this view took the gesture. */
        private boolean track(MotionEvent e) {
            if (velocity == null) velocity = VelocityTracker.obtain();
            velocity.addMovement(e);
            detector.onTouchEvent(e);
            float fx = focus(e, true), fy = focus(e, false);
            switch (e.getActionMasked()) {
                case MotionEvent.ACTION_DOWN:
                    // A touch catches a glide.
                    boolean gliding = !glide.isFinished();
                    stop();
                    panning = gliding;
                    yielding = false;
                    downX = lastX = fx;
                    downY = lastY = fy;
                    // A scroll view around (a pager) intercepts at the same slop, before this view sees the move.
                    deciding = !panning && (maxX() > 0 || maxY() > 0);
                    if (deciding) getParent().requestDisallowInterceptTouchEvent(true);
                    break;
                case MotionEvent.ACTION_POINTER_DOWN:
                case MotionEvent.ACTION_POINTER_UP:
                    lastX = fx;
                    lastY = fy;
                    if (pinch && e.getPointerCount() > 1) startPan();
                    break;
                case MotionEvent.ACTION_MOVE: {
                    if (!panning) {
                        float dx = fx - downX, dy = fy - downY;
                        if (deciding && (Math.abs(dx) > slop || Math.abs(dy) > slop)) {
                            deciding = false;
                            if (!pans(dx, dy)) {
                                // Nothing to move this way: the views around get the next move.
                                getParent().requestDisallowInterceptTouchEvent(false);
                                break;
                            }
                        }
                        boolean along = (Math.abs(dx) > slop && (maxX() > 0 || bounces))
                                || (Math.abs(dy) > slop && (maxY() > 0 || bounces));
                        if (along && (maxX() > 0 || maxY() > 0 || scale != 1)) {
                            startPan();
                            lastX = fx;
                            lastY = fy;
                        }
                    }
                    if (panning) {
                        float mx = lastX - fx, my = lastY - fy;
                        boolean out = e.getPointerCount() == 1 && !detector.isInProgress()
                                && (yielding ? !inward(mx) : handsOver(mx, my));
                        if (out != yielding) {
                            yielding = out;
                            getParent().requestDisallowInterceptTouchEvent(!out);
                        }
                        // Handed over from the edge, not from the rubber band past it: a cancel does not spring back.
                        if (out) scrollTo(clamp(getScrollX(), maxX()), getScrollY());
                        panBy(out ? 0 : mx, my);
                        lastX = fx;
                        lastY = fy;
                    }
                    break;
                }
                default:
                    break;
            }
            return panning;
        }

        /** Whether a finger's drag by ({@code dx}, {@code dy}) has content to move along its main axis. */
        private boolean pans(float dx, float dy) {
            if (Math.abs(dx) >= Math.abs(dy)) return dx < 0 ? getScrollX() < maxX() : getScrollX() > 0;
            return dy < 0 ? getScrollY() < maxY() : getScrollY() > 0;
        }

        /** Whether a horizontal scroll by {@code mx} goes back into the content. */
        private boolean inward(float mx) {
            return mx > 0 ? getScrollX() < maxX() : mx < 0 && getScrollX() > 0;
        }

        /** Whether a scroll by ({@code mx}, {@code my}) pushes past a horizontal edge where a scroll view around can go on. */
        private boolean handsOver(float mx, float my) {
            if (Math.abs(mx) <= Math.abs(my)) return false;
            if (mx > 0 ? getScrollX() < maxX() : getScrollX() > 0) return false;
            for (ViewParent p = ScrollBox.this.getParent(); p != null; p = p.getParent()) {
                if (p instanceof ScrollBox && ((ScrollBox) p).scrolls(-mx, -my)) return true;
            }
            return false;
        }

        /** Scrolls by a finger's move, resisting past the edges (or stopping there). */
        private void panBy(float dx, float dy) {
            int x = getScrollX(), y = getScrollY();
            scrollTo(x + Math.round(resist(x, dx, maxX())), y + Math.round(resist(y, dy, maxY())));
        }

        private float resist(int at, float d, int max) {
            float next = at + d;
            if (next >= 0 && next <= max) return d;
            if (!bounces) return Math.max(0, Math.min(max, next)) - at;
            return d / 2;
        }

        /** The fingers lifted: settle the zoom, then bounce back or glide. */
        private void release() {
            panning = false;
            float vx = 0, vy = 0;
            if (velocity != null) {
                velocity.computeCurrentVelocity(1000, maxFling);
                vx = velocity.getXVelocity();
                vy = velocity.getYVelocity();
                velocity.recycle();
                velocity = null;
            }
            if (scale > maxZoom || scale < minZoom) {
                zoomBack();
                return;
            }
            int x = getScrollX(), y = getScrollY(), mx = maxX(), my = maxY();
            if (x < 0 || y < 0 || x > mx || y > my) {
                glide.springBack(x, y, 0, mx, 0, my);
                postInvalidateOnAnimation();
            } else if (Math.hypot(vx, vy) > minFling) {
                int over = bounces ? Math.round(48 * mounter.density) : 0;
                glide.fling(x, y, Math.round(-vx), Math.round(-vy), 0, mx, 0, my, over, over);
                flung = true;
                postInvalidateOnAnimation();
            }
        }

        /** A pinch ended beyond the limits: back to the nearest one. */
        private void zoomBack() {
            float to = Math.max(minZoom, Math.min(maxZoom, scale));
            float from = scale, fx = lastX, fy = lastY;
            float px = (getScrollX() + fx - getPaddingLeft() - offsetX(scale)) / scale;
            float py = (getScrollY() + fy - getPaddingTop() - offsetY(scale)) / scale;
            zoomAnimation = android.animation.ValueAnimator.ofFloat(0, 1);
            zoomAnimation.setDuration(250);
            zoomAnimation.setInterpolator(new android.view.animation.DecelerateInterpolator());
            zoomAnimation.addUpdateListener(a -> {
                float t = (float) a.getAnimatedValue();
                place(from + (to - from) * t, px, py, fx, fy);
            });
            zoomAnimation.start();
        }

        @Override
        public void computeScroll() {
            if (glide.computeScrollOffset()) {
                scrollTo(glide.getCurrX(), glide.getCurrY());
                postInvalidateOnAnimation();
            }
        }

        @Override
        public boolean onInterceptTouchEvent(MotionEvent e) {
            if (!enabled || jsBlocked || gateBlocked) return false;
            return track(e);
        }

        @SuppressLint("ClickableViewAccessibility")
        @Override
        public boolean onTouchEvent(MotionEvent e) {
            if (!enabled || jsBlocked || gateBlocked) return false;
            boolean up = beforeTouch(e);
            int action = e.getActionMasked();
            // A down that reaches here was seen by the interception already.
            if (action != MotionEvent.ACTION_DOWN) track(e);
            if (up) {
                if (action == MotionEvent.ACTION_UP) release();
                else {
                    panning = false;
                    if (scale > maxZoom || scale < minZoom) zoomBack();
                }
                released();
            }
            return true;
        }
    }

    // --- Pull to refresh -------------------------------------------------

    private boolean canPull() {
        return !horizontal && !zooms && (events & (1L << Native.REFRESH)) != 0 && scroller.getScrollY() == 0;
    }

    @Override
    public boolean dispatchTouchEvent(MotionEvent e) {
        int action = e.getActionMasked();
        touching = action != MotionEvent.ACTION_UP && action != MotionEvent.ACTION_CANCEL;
        if (action == MotionEvent.ACTION_DOWN) {
            // The user scrolls now: what was owed no longer applies.
            owedX = owedY = 0;
            dragging = false;
            tapMode = tapMode(e);
            // "never": the touch goes on as usual; a tap only dismisses the keyboard (Mounter.keyboardTap).
            if (tapMode == 0) mounter.startTapOnly(this);
            tapX = e.getRawX();
            tapY = e.getRawY();
            tapMoved = false;
        }
        if (action == MotionEvent.ACTION_MOVE && tapMode >= 0) moved(e);
        if (action == MotionEvent.ACTION_MOVE && dismissMode != 0) {
            float slop = ViewConfiguration.get(getContext()).getScaledTouchSlop();
            if (Math.abs(e.getY() - startY) > slop) hideKeyboard();
        }
        if (action == MotionEvent.ACTION_DOWN) startY = e.getY();
        boolean handled;
        dispatching = true;
        try {
            handled = super.dispatchTouchEvent(e);
        } finally {
            dispatching = false;
        }
        // For a pan around this view (Box.scrollTakes): a touch that a view
        // above took never comes here, and leaves the time of an older one.
        if (action == MotionEvent.ACTION_DOWN) downTime = handled ? e.getDownTime() : -1;
        if (action == MotionEvent.ACTION_UP && tapMode == 2 && !dragging && !tapMoved
                && !handlesTap(this, e.getX(), e.getY())) {
            hideKeyboard();
        }
        if (!touching) {
            if (tapMode == 0) mounter.endTapOnly(this);
            tapMode = -1;
        }
        return handled;
    }

    private void moved(MotionEvent e) {
        float slop = ViewConfiguration.get(getContext()).getScaledTouchSlop();
        if (Math.hypot(e.getRawX() - tapX, e.getRawY() - tapY) > slop) tapMoved = true;
    }

    /** Whether the touch, ending with {@code up} (in a root's coordinates), is a tap: quick, and it moved nothing. */
    boolean stillTap(MotionEvent up) {
        moved(up);
        return !dragging && !tapMoved && up.getEventTime() - up.getDownTime() < ViewConfiguration.getLongPressTimeout();
    }

    /**
     * How a touch treats the keyboard ({@code keyboardShouldPersistTaps}):
     * -1 when no input is focused, the keyboard is down (the back button
     * hides it and leaves the input focused) or the touch is on an input,
     * else the mode of the outermost scroll view that does not let taps
     * through.
     */
    private int tapMode(MotionEvent e) {
        if (mounter.focusedInput() == null || !keyboardShown() || inputAt(this, e.getX(), e.getY())) return -1;
        for (ViewParent p = getParent(); p != null; p = p.getParent()) {
            if (p instanceof ScrollBox && ((ScrollBox) p).keyboardTaps != 1) return -1;
        }
        return keyboardTaps == 1 ? -1 : keyboardTaps;
    }

    /**
     * Whether the soft keyboard shows. Before Android 11 the window does not
     * say: a focused input is taken to have it up.
     */
    private boolean keyboardShown() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return true;
        WindowInsets insets = getRootWindowInsets();
        return insets == null || insets.isVisible(WindowInsets.Type.ime());
    }

    /**
     * Whether an input is under ({@code x}, {@code y}) of {@code group}: in
     * any child there, topmost first, as a touch a child does not take goes
     * on to the one below (a decoration over a field does not hide it).
     */
    private static boolean inputAt(ViewGroup group, float x, float y) {
        for (int i = group.getChildCount() - 1; i >= 0; i--) {
            View c = group.getChildAt(i);
            if (c.getVisibility() != VISIBLE) continue;
            float lx = localX(group, c, x), ly = localY(group, c, y);
            if (lx < 0 || ly < 0 || lx >= c.getWidth() || ly >= c.getHeight()) continue;
            if (c instanceof InputBox) return true;
            if (c instanceof ViewGroup && inputAt((ViewGroup) c, lx, ly)) return true;
        }
        return false;
    }

    /** Whether something under ({@code x}, {@code y}) of {@code group} handles taps. */
    private static boolean handlesTap(ViewGroup group, float x, float y) {
        View v = childAt(group, x, y);
        if (v == null) return false;
        if (v.isClickable() || v.isLongClickable() || v instanceof InputBox || v instanceof android.widget.SeekBar) return true;
        return v instanceof ViewGroup && handlesTap((ViewGroup) v, localX(group, v, x), localY(group, v, y));
    }

    /** The topmost visible child of {@code group} under ({@code x}, {@code y}). */
    private static View childAt(ViewGroup group, float x, float y) {
        for (int i = group.getChildCount() - 1; i >= 0; i--) {
            View c = group.getChildAt(i);
            if (c.getVisibility() != VISIBLE) continue;
            float lx = localX(group, c, x), ly = localY(group, c, y);
            if (lx >= 0 && ly >= 0 && lx < c.getWidth() && ly < c.getHeight()) return c;
        }
        return null;
    }

    private static float localX(ViewGroup group, View child, float x) {
        return x + group.getScrollX() - child.getLeft() - child.getTranslationX();
    }

    private static float localY(ViewGroup group, View child, float y) {
        return y + group.getScrollY() - child.getTop() - child.getTranslationY();
    }

    void hideKeyboard() {
        // The focused field is usually outside this scroll view (a chat's composer below its list).
        View focused = getRootView().findFocus();
        if (focused == null) focused = mounter.focusedInput();
        if (focused == null) return;
        InputMethodManager imm = (InputMethodManager) getContext().getSystemService(Context.INPUT_METHOD_SERVICE);
        imm.hideSoftInputFromWindow(focused.getWindowToken(), 0);
        mounter.host.requestFocus();
    }

    @Override
    public boolean onInterceptTouchEvent(MotionEvent e) {
        if (refreshing || !canPull()) return false;
        if (e.getActionMasked() == MotionEvent.ACTION_MOVE) {
            float dy = e.getY() - startY;
            float slop = ViewConfiguration.get(getContext()).getScaledTouchSlop();
            if (dy > slop) {
                pulling = true;
                startY = e.getY();
                return true;
            }
        }
        return false;
    }

    @SuppressLint("ClickableViewAccessibility")
    @Override
    public boolean onTouchEvent(MotionEvent e) {
        if (!pulling) return false;
        float d = mounter.density;
        switch (e.getActionMasked()) {
            case MotionEvent.ACTION_MOVE:
                // Resistance like a rubber band.
                setPull(Math.max(0, (e.getY() - startY) * 0.5f));
                if (refreshIndicator) {
                    spinner.setVisibility(VISIBLE);
                    spinner.setAlpha(Math.min(1, pull / (refreshThreshold * d)));
                }
                return true;
            case MotionEvent.ACTION_UP:
            case MotionEvent.ACTION_CANCEL:
                pulling = false;
                if (pull >= refreshThreshold * d) {
                    setRefreshing(true);
                    mounter.event(id, Native.REFRESH);
                } else {
                    animatePull(0, () -> spinner.setVisibility(GONE));
                }
                return true;
        }
        return true;
    }

    void setRefreshing(boolean on) {
        if (on == refreshing) return;
        refreshing = on;
        float d = mounter.density;
        if (on) {
            if (refreshIndicator) {
                spinner.setVisibility(VISIBLE);
                spinner.setAlpha(1);
            }
            animatePull(refreshInset * d, null);
        } else {
            animatePull(0, () -> spinner.setVisibility(GONE));
        }
    }

    private void setPull(float px) {
        if (px == pull) return;
        pull = px;
        scroller.setTranslationY(px);
        spinner.setTranslationY(Math.max(0, px / 2 - spinner.getHeight() / 2f));
        // The pull is a translation, not a scroll: the offset stays 0.
        mounter.pull(id, px / mounter.density);
    }

    private void animatePull(float to, Runnable end) {
        android.animation.ValueAnimator a = android.animation.ValueAnimator.ofFloat(pull, to);
        a.setDuration(250);
        a.addUpdateListener(v -> setPull((float) v.getAnimatedValue()));
        if (end != null) {
            a.addListener(new android.animation.AnimatorListenerAdapter() {
                @Override
                public void onAnimationEnd(android.animation.Animator animation) {
                    end.run();
                }
            });
        }
        a.start();
    }
}
