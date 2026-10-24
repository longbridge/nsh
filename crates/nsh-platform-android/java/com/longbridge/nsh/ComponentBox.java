package com.longbridge.nsh;

import android.annotation.SuppressLint;
import android.content.Context;

/** The box of a registered native component: laid out by the core, filled by the component's view. */
@SuppressLint("ViewConstructor")
final class ComponentBox extends Box {
    NshComponent component;

    ComponentBox(Context context, Mounter mounter, int id) {
        super(context, mounter, id);
    }

    void attach(NshComponent component) {
        this.component = component;
        // Its own child, at the box's size (the core places no children here).
        addView(component.view());
    }

    /** The element is gone, or its app stopped: tells the component once. */
    void destroy() {
        NshComponent c = component;
        if (c == null) return;
        // Cleared first, so it is told once even if destroy() throws.
        component = null;
        try {
            c.destroy();
        } finally {
            removeAllViews();
        }
    }

    @Override
    protected void onMeasure(int widthSpec, int heightSpec) {
        int w = MeasureSpec.getSize(widthSpec), h = MeasureSpec.getSize(heightSpec);
        setMeasuredDimension(w, h);
        int exactW = MeasureSpec.makeMeasureSpec(w, MeasureSpec.EXACTLY);
        int exactH = MeasureSpec.makeMeasureSpec(h, MeasureSpec.EXACTLY);
        for (int i = 0; i < getChildCount(); i++) getChildAt(i).measure(exactW, exactH);
    }

    @Override
    protected void onLayout(boolean changed, int l, int t, int r, int b) {
        for (int i = 0; i < getChildCount(); i++) getChildAt(i).layout(0, 0, r - l, b - t);
    }
}
