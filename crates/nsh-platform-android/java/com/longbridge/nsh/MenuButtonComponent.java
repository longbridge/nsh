package com.longbridge.nsh;

import android.content.Context;
import android.view.View;
import android.widget.Button;
import android.widget.PopupMenu;
import java.util.ArrayList;
import java.util.List;
import java.util.function.BooleanSupplier;

/**
 * {@code nsh/base}'s {@code MenuButton} on Android: a transparent button over
 * the app's content whose tap (or TalkBack's activation, or Enter on it)
 * opens the system's popup menu below the content at once, without asking
 * JS ({@link SystemMenus}). A chosen action emits {@code select} with the
 * app's id. The button is what TalkBack reaches (named by
 * {@code accessibilityLabel}): the content under it is drawn, not read twice
 * ({@link Box#addChildrenForAccessibility}).
 */
final class MenuButtonComponent implements NshComponent {
    /** Its name ({@code nsh_core::platform::MENU_BUTTON_COMPONENT}). */
    static final String NAME = "nsh/menu-button";

    private final Context context;
    private final NshComponentContext events;
    private final BooleanSupplier night;
    private final View button;
    private List<SystemMenus.Entry> items = new ArrayList<>();
    private String title;
    /** The menu shown (null: none). */
    private PopupMenu shown;

    MenuButtonComponent(Context context, NshComponentContext events, BooleanSupplier night) {
        this.context = context;
        this.events = events;
        this.night = night;
        button = new View(context) {
            @Override
            public CharSequence getAccessibilityClassName() {
                return Button.class.getName();
            }
        };
        button.setClickable(true);
        button.setFocusable(true);
        button.setOnClickListener(v -> show());
    }

    @Override
    public View view() {
        return button;
    }

    @Override
    public void setProp(String name, Object value) {
        switch (name) {
            case "items":
                // New items change the menu the next time it opens.
                items = SystemMenus.parse(value);
                break;
            case "title":
                title = value instanceof String ? (String) value : null;
                break;
            case "disabled":
                button.setEnabled(!Boolean.TRUE.equals(value));
                break;
            case "accessibilityLabel":
                button.setContentDescription(value instanceof String ? (String) value : null);
                break;
            default:
                break;
        }
    }

    private void show() {
        if (shown != null || items.isEmpty() || !button.isAttachedToWindow()) return;
        PopupMenu popup = SystemMenus.popup(SystemMenus.themed(context, night.getAsBoolean()), button,
                SystemMenus.gravity(button), title, items, id -> events.emit("select", id));
        popup.setOnDismissListener(p -> {
            if (shown == p) shown = null;
        });
        shown = popup;
        popup.show();
    }

    @Override
    public void destroy() {
        PopupMenu popup = shown;
        shown = null;
        if (popup != null) popup.dismiss();
    }
}
