package com.longbridge.nsh;

import android.content.Context;
import android.content.res.TypedArray;
import android.graphics.Bitmap;
import android.graphics.PorterDuff;
import android.graphics.drawable.BitmapDrawable;
import android.graphics.drawable.Drawable;
import android.os.Build;
import android.text.SpannableString;
import android.text.Spanned;
import android.text.style.ForegroundColorSpan;
import android.text.style.RelativeSizeSpan;
import android.util.LruCache;
import android.view.ContextThemeWrapper;
import android.view.Gravity;
import android.view.Menu;
import android.view.MenuItem;
import android.view.SubMenu;
import android.view.View;
import android.widget.PopupMenu;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.function.Consumer;

/**
 * The system's popup menu ({@link PopupMenu}) of {@code nsh/base}'s
 * {@code MenuButton} and of its {@code ContextMenu} given {@code items}
 * ({@link MenuButtonComponent}, {@link ContextMenuComponent}): made from the
 * items' data, in the theme of the app's appearance. Main thread.
 *
 * <p>A title goes first, as an item that cannot be chosen, in the theme's
 * secondary text color: {@code PopupMenu} has no title of its own. TalkBack
 * reads it as a disabled item, not as a heading: its rows are the
 * framework's, which no public API reaches.
 */
final class SystemMenus {
    private SystemMenus() {}

    /** One item, as the {@code items} prop gives it ({@code nsh_base::menu_button::menu_json}). */
    static final class Entry {
        String id = "";
        String label = "";
        /** Its icon, an SVG document (null: none). */
        String svg;
        /** Checkable, checked or not (null: not checkable). */
        Boolean checked;
        boolean disabled, destructive, inline;
        final List<Entry> items = new ArrayList<>();
    }

    /** How deep submenus nest: past it an entry's items are dropped, as on iOS. */
    private static final int MAX_DEPTH = 8;

    /** The entries of an {@code items} prop; anything but objects is skipped. */
    static List<Entry> parse(Object value) {
        return parse(value, 0);
    }

    private static List<Entry> parse(Object value, int depth) {
        List<Entry> entries = new ArrayList<>();
        if (!(value instanceof List)) return entries;
        for (Object o : (List<?>) value) {
            if (!(o instanceof Map)) continue;
            Map<?, ?> m = (Map<?, ?>) o;
            Entry e = new Entry();
            e.id = text(m.get("id"));
            e.label = text(m.get("label"));
            e.svg = m.get("svg") instanceof String ? (String) m.get("svg") : null;
            e.checked = m.get("checked") instanceof Boolean ? (Boolean) m.get("checked") : null;
            e.disabled = Boolean.TRUE.equals(m.get("disabled"));
            e.destructive = Boolean.TRUE.equals(m.get("destructive"));
            e.inline = Boolean.TRUE.equals(m.get("inline"));
            if (depth + 1 < MAX_DEPTH) e.items.addAll(parse(m.get("items"), depth + 1));
            entries.add(e);
        }
        return entries;
    }

    private static String text(Object o) {
        return o instanceof String ? (String) o : "";
    }

    /** {@code context} in the system's theme of the app's appearance. */
    static Context themed(Context context, boolean night) {
        return new ContextThemeWrapper(context,
                night ? android.R.style.Theme_DeviceDefault : android.R.style.Theme_DeviceDefault_Light);
    }

    /**
     * Where a menu anchored to {@code anchor} lines up: its end edge when the
     * anchor's middle is in the end half of the window (an overflow button at
     * the top end), its start edge otherwise.
     */
    static int gravity(View anchor) {
        int[] at = new int[2];
        anchor.getLocationInWindow(at);
        int middle = at[0] + anchor.getWidth() / 2;
        int width = anchor.getRootView().getWidth();
        boolean rtl = anchor.getLayoutDirection() == View.LAYOUT_DIRECTION_RTL;
        boolean right = middle > width / 2;
        return right != rtl ? Gravity.END : Gravity.START;
    }

    /**
     * A popup menu of {@code entries} below {@code anchor}, with {@code title}
     * first when there is one; a chosen action calls {@code select} with its
     * id. The caller shows it and hears its dismissal.
     */
    static PopupMenu popup(Context themed, View anchor, int gravity, String title, List<Entry> entries,
            Consumer<String> select) {
        PopupMenu popup = new PopupMenu(themed, anchor, gravity);
        Menu menu = popup.getMenu();
        Builder builder = new Builder(themed);
        if (title != null && !title.isEmpty()) {
            MenuItem header = menu.add(builder.group++, Menu.NONE, builder.order++, builder.header(title));
            header.setEnabled(false);
        }
        builder.add(menu, entries);
        // Icons only show when asked (API 29); earlier the menu is text only.
        if (builder.icons && Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) popup.setForceShowIcon(true);
        popup.setOnMenuItemClickListener(item -> {
            String id = builder.ids.get(item.getItemId());
            if (id != null) select.accept(id);
            return id != null;
        });
        return popup;
    }

    /** Fills a menu: item ids map to the app's, groups split the sections. */
    private static final class Builder {
        final Context context;
        final Map<Integer, String> ids = new HashMap<>();
        final int iconPx;
        final int textColor, secondaryColor, errorColor;
        int group = 1, nextId = 1, order = 0;
        boolean icons;

        Builder(Context context) {
            this.context = context;
            this.iconPx = Math.round(24 * context.getResources().getDisplayMetrics().density);
            this.textColor = color(context, android.R.attr.textColorPrimary, 0xff1d1b20);
            this.secondaryColor = color(context, android.R.attr.textColorSecondary, 0xff49454f);
            // Material 3's error color where the theme has none (before API 26).
            this.errorColor = Build.VERSION.SDK_INT >= Build.VERSION_CODES.O
                    ? color(context, android.R.attr.colorError, 0xffb3261e)
                    : 0xffb3261e;
        }

        /**
         * Adds {@code entries} to {@code menu}: an inline section's items in
         * a group of their own (a divider between groups from API 28).
         * Checkable items are checks, as on iOS, never inferred as a single
         * choice.
         */
        void add(Menu menu, List<Entry> entries) {
            int current = group++;
            boolean afterSection = false;
            for (Entry e : entries) {
                if (e.items.isEmpty()) {
                    if (afterSection) {
                        current = group++;
                        afterSection = false;
                    }
                    action(menu, current, e);
                } else if (e.inline) {
                    int section = group++;
                    if (!e.label.isEmpty()) {
                        menu.add(section, Menu.NONE, order++, header(e.label)).setEnabled(false);
                    }
                    for (Entry item : e.items) {
                        if (item.items.isEmpty()) action(menu, section, item);
                        else submenu(menu, section, item);
                    }
                    afterSection = true;
                } else {
                    if (afterSection) {
                        current = group++;
                        afterSection = false;
                    }
                    submenu(menu, current, e);
                }
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) menu.setGroupDividerEnabled(true);
        }

        /** A submenu in {@code group}: a section's stays in the section, as on iOS. */
        private void submenu(Menu menu, int group, Entry e) {
            SubMenu sub = menu.addSubMenu(group, nextId++, order++, label(e));
            sub.setHeaderTitle(e.label);
            sub.getItem().setEnabled(!e.disabled);
            Drawable icon = icon(e);
            if (icon != null) sub.getItem().setIcon(icon);
            add(sub, e.items);
        }

        private void action(Menu menu, int group, Entry e) {
            int id = nextId++;
            MenuItem item = menu.add(group, id, order++, label(e));
            ids.put(id, e.id);
            item.setEnabled(!e.disabled);
            if (e.checked != null) item.setCheckable(true).setChecked(e.checked);
            Drawable icon = icon(e);
            if (icon != null) item.setIcon(icon);
        }

        /** A title row's text: secondary, a step smaller. */
        CharSequence header(String title) {
            SpannableString s = new SpannableString(title);
            s.setSpan(new ForegroundColorSpan(secondaryColor), 0, s.length(), Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
            s.setSpan(new RelativeSizeSpan(0.875f), 0, s.length(), Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
            return s;
        }

        /** An item's label: in the error color when it is destructive. */
        private CharSequence label(Entry e) {
            if (!e.destructive || e.label.isEmpty()) return e.label;
            SpannableString s = new SpannableString(e.label);
            s.setSpan(new ForegroundColorSpan(errorColor), 0, s.length(), Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
            return s;
        }

        /**
         * An item's SVG icon in its text color; null without one, before
         * API 29 or when it does not draw.
         */
        private Drawable icon(Entry e) {
            if (e.svg == null || e.svg.isEmpty() || Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) return null;
            Bitmap bitmap = bitmap(e.svg, iconPx);
            if (bitmap == null) return null;
            BitmapDrawable d = new BitmapDrawable(context.getResources(), bitmap);
            // Its own aspect, its longer side the icon's size.
            float scale = (float) iconPx / Math.max(bitmap.getWidth(), bitmap.getHeight());
            d.setBounds(0, 0, Math.round(bitmap.getWidth() * scale), Math.round(bitmap.getHeight() * scale));
            d.setTint(e.destructive ? errorColor : textColor);
            d.setTintMode(PorterDuff.Mode.SRC_IN);
            if (e.disabled) d.setAlpha(97); // Material's disabled content: 38%.
            icons = true;
            return d;
        }
    }

    /** Rasterized icons by document and size: a menu reopened draws the same ones. */
    private static final LruCache<String, Bitmap> BITMAPS = new LruCache<>(64);

    /**
     * The SVG {@code svg} drawn with its shorter side {@code px} pixels long;
     * null when it is no SVG or SVGs are not built in.
     */
    private static Bitmap bitmap(String svg, int px) {
        String key = px + ":" + svg;
        Bitmap cached = BITMAPS.get(key);
        if (cached != null) return cached;
        int[] size = new int[4];
        byte[] pixels;
        try {
            float density = android.content.res.Resources.getSystem().getDisplayMetrics().density;
            pixels = Native.svg(svg.getBytes(java.nio.charset.StandardCharsets.UTF_8), px, density, size,
                    new float[2]);
        } catch (RuntimeException | LinkageError e) {
            return null;
        }
        if (pixels == null || size[0] <= 0 || size[1] <= 0) return null;
        Bitmap bitmap = Bitmap.createBitmap(size[0], size[1], Bitmap.Config.ARGB_8888);
        bitmap.copyPixelsFromBuffer(java.nio.ByteBuffer.wrap(pixels));
        BITMAPS.put(key, bitmap);
        return bitmap;
    }

    private static int color(Context context, int attr, int fallback) {
        TypedArray a = context.obtainStyledAttributes(new int[] {attr});
        try {
            return a.getColor(0, fallback);
        } finally {
            a.recycle();
        }
    }
}
