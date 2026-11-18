package com.longbridge.nsh;

/**
 * Which children a scroll view's content draws ({@code ScrollBox.Content}),
 * kept free of Android types so it is testable on a plain JDK. A
 * virtualized list keeps a viewport of rows bound on either side of the one
 * shown; drawn, the render thread walks and records them all every frame.
 * The content leaves out its direct children (rows) wholly outside the
 * viewport and a margin around it, and draws again before the viewport comes
 * within half that margin of what it left out: hardware scrolling replays
 * the content's display list without drawing it, so what it left out stays
 * out until then.
 *
 * <p>What a row may draw in is its reach ({@link #reach}): its bounds, what
 * its descendants cover outside them and what its own decoration draws
 * outside them ({@link #paint}). What a descendant draws beyond its own
 * bounds is covered by half the margin, at least {@link #MIN_MARGIN_DP} / 2;
 * a descendant transformed further than {@link #PIN_EXCESS_DP} beyond its
 * bounds keeps its row drawn ({@link #pins}).
 *
 * <p>Rectangles are in the content's pixels, edges exclusive at the right
 * and bottom.
 */
final class ScrollCull {
    private ScrollCull() {}

    /** The least margin, in dp. */
    static final int MIN_MARGIN_DP = 64;

    /**
     * How far a descendant's transform may take it beyond its own bounds
     * (dp) before its row is kept drawn: half the coverage half the least
     * margin gives, leaving the rest to what it draws outside them (a
     * shadow).
     */
    static final int PIN_EXCESS_DP = 16;

    /**
     * How far a box shadow's blur reaches beyond its spread, per px of CSS
     * blur: the shadow's deviation is half the blur and its visible edge
     * three deviations out.
     */
    static final float SHADOW_BLUR_REACH = 1.5f;

    /**
     * How far a Material shadow reaches beyond the view, per px of its z
     * (elevation and translation z): the spot shadow's offset and blur grow
     * with z and with the view's distance from the light.
     */
    static final float ELEVATION_REACH = 3f;

    /**
     * How far beyond a {@code width} × {@code height} px viewport children
     * are drawn (px): a quarter of its larger side, at least
     * {@link #MIN_MARGIN_DP}. It takes in a list's rows near the viewport,
     * what a row's descendants draw outside its reach and the next frames of
     * a scroll.
     */
    static int margin(int width, int height, float density) {
        return Math.max(Math.round(MIN_MARGIN_DP * density), Math.max(width, height) / 4);
    }

    /**
     * What a view's decoration draws outside its bounds, as outward extents
     * (px, left, top, right, bottom, never negative) into {@code out}; false
     * when it draws nothing outside them. Outer box shadows ({@code shadows}:
     * dx, dy, blur, spread, inset per layer, as {@code Decor} holds them,
     * with their {@code colors}; transparent and inset ones draw inside), an
     * outline (width and offset; none when transparent), a Material shadow
     * for {@code z} and a shader's {@code shaderOverflow}.
     */
    static boolean paint(float[] shadows, int[] colors, float outlineWidth, int outlineColor, float outlineOffset,
            float z, int shaderOverflow, float[] out) {
        float l = 0, t = 0, r = 0, b = 0;
        int layers = shadows == null || colors == null ? 0 : Math.min(colors.length, shadows.length / 5);
        for (int n = 0; n < layers; n++) {
            int i = n * 5;
            if (shadows[i + 4] != 0 || colors[n] >>> 24 == 0) continue;
            float dx = shadows[i], dy = shadows[i + 1];
            float e = shadows[i + 3] + Math.max(0, shadows[i + 2]) * SHADOW_BLUR_REACH;
            l = Math.max(l, e - dx);
            r = Math.max(r, e + dx);
            t = Math.max(t, e - dy);
            b = Math.max(b, e + dy);
        }
        float all = 0;
        if (outlineWidth > 0 && outlineColor >>> 24 != 0) all = Math.max(all, outlineOffset + outlineWidth);
        if (z > 0) all = Math.max(all, z * ELEVATION_REACH);
        if (shaderOverflow > 0) all = Math.max(all, shaderOverflow);
        out[0] = Math.max(l, all);
        out[1] = Math.max(t, all);
        out[2] = Math.max(r, all);
        out[3] = Math.max(b, all);
        return out[0] > 0 || out[1] > 0 || out[2] > 0 || out[3] > 0;
    }

    /**
     * The rectangle a child at {@code left, top, right, bottom} may draw in,
     * into {@code out}: its bounds, widened by what its descendants cover
     * ({@code overflow}: left, top, right, bottom in its own px, or null;
     * negative where they reach above or left of it) and by what it draws
     * outside them ({@code paint}: outward extents from {@link #paint}, or
     * null), rounded outwards to whole pixels.
     */
    static void reach(int left, int top, int right, int bottom, float[] overflow, float[] paint, int[] out) {
        int l = left, t = top, r = right, b = bottom;
        if (overflow != null) {
            l = Math.min(l, left + (int) Math.floor(overflow[0]));
            t = Math.min(t, top + (int) Math.floor(overflow[1]));
            r = Math.max(r, left + (int) Math.ceil(overflow[2]));
            b = Math.max(b, top + (int) Math.ceil(overflow[3]));
        }
        if (paint != null) {
            l = Math.min(l, left - (int) Math.ceil(paint[0]));
            t = Math.min(t, top - (int) Math.ceil(paint[1]));
            r = Math.max(r, right + (int) Math.ceil(paint[2]));
            b = Math.max(b, bottom + (int) Math.ceil(paint[3]));
        }
        out[0] = l;
        out[1] = t;
        out[2] = r;
        out[3] = b;
    }

    /**
     * Whether a child reaching {@code reach} (left, top, right, bottom) lies
     * wholly outside the viewport {@code vl, vt, vr, vb} widened by
     * {@code margin}.
     */
    static boolean outside(int[] reach, int vl, int vt, int vr, int vb, int margin) {
        return outside(reach[0], reach[1], reach[2], reach[3], vl, vt, vr, vb, margin);
    }

    /**
     * Whether a child covering {@code left, top, right, bottom} lies wholly
     * outside the viewport {@code vl, vt, vr, vb} widened by {@code margin}.
     */
    static boolean outside(int left, int top, int right, int bottom, int vl, int vt, int vr, int vb, int margin) {
        return right < vl - margin || left > vr + margin || bottom < vt - margin || top > vb + margin;
    }

    /**
     * Whether a view stands where its bounds say: no translation, scale or
     * rotation, about any pivot. A row that does not may slide into the
     * viewport while the content's display list is replayed without it; a
     * mirrored cell (scale x -1, a right-to-left horizontal list's) is drawn
     * too.
     */
    static boolean identity(float translationX, float translationY, float scaleX, float scaleY, float rotation,
            float rotationX, float rotationY) {
        return translationX == 0 && translationY == 0 && scaleX == 1 && scaleY == 1 && rotation == 0
                && rotationX == 0 && rotationY == 0;
    }

    /**
     * How far a {@code width} × {@code height} px view's transform takes it
     * beyond its own bounds (px): its transformed bounds are
     * {@code left, top, right, bottom} in its untransformed px. NaN when the
     * transform does not map them (a 3D rotation past the camera).
     */
    static float excess(float left, float top, float right, float bottom, int width, int height) {
        if (Float.isNaN(left) || Float.isNaN(top) || Float.isNaN(right) || Float.isNaN(bottom)) return Float.NaN;
        return Math.max(Math.max(0, Math.max(-left, -top)), Math.max(right - width, bottom - height));
    }

    /**
     * Whether a descendant whose transform takes it {@code excess} px beyond
     * its bounds keeps its row drawn: past {@code limit} px (from
     * {@link #PIN_EXCESS_DP}), or wherever it cannot be told. A descendant
     * under a clipping ancestor within the row draws inside that ancestor,
     * and never pins.
     */
    static boolean pins(float excess, int limit, boolean clipped) {
        return !clipped && !(excess <= limit);
    }

    /**
     * Whether children drawn for the viewport {@code drawn} (left, top,
     * right, bottom) and {@code margin} must be drawn again for the viewport
     * {@code vl, vt, vr, vb}: an edge of it went more than half the margin
     * past the drawn one. Within that, every child within half the margin of
     * the viewport is still drawn; a viewport that shrinks or moves back
     * needs nothing.
     */
    static boolean stale(int[] drawn, int margin, int vl, int vt, int vr, int vb) {
        int half = margin / 2;
        return vl < drawn[0] - half || vt < drawn[1] - half || vr > drawn[2] + half || vb > drawn[3] + half;
    }
}
