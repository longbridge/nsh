package com.longbridge.nsh;

/** JNI entry points of libnsh.so (crates/nsh-platform-android/src/host.rs). */
final class Native {
    static {
        System.loadLibrary("nsh");
    }

    private Native() {}

    // Event kinds, in `nsh_core::EventKind` order.
    static final int PRESS = 0;
    static final int LONG_PRESS = 1;
    static final int PAN_START = 2;
    static final int PAN_END = 3;
    static final int REFRESH = 4;
    static final int LOAD_MORE = 5;
    static final int SCROLL_END = 6;
    static final int LOAD = 7;
    static final int ERROR = 8;
    static final int CHANGE_TEXT = 9;
    static final int SUBMIT_EDITING = 10;
    static final int FOCUS = 11;
    static final int BLUR = 12;
    static final int PRESS_IN = 13;
    static final int PRESS_OUT = 14;
    static final int LAYOUT = 15;
    static final int PINCH_START = 16;
    static final int PINCH_END = 17;
    static final int ROTATION_START = 18;
    static final int ROTATION_END = 19;
    static final int ACCESSIBILITY_ACTION = 20;
    static final int DISMISS = 21;
    static final int REQUEST_CLOSE = 22;
    static final int SHOW = 23;
    static final int INSETS = 24;
    static final int LOAD_START = 25;
    static final int MESSAGE = 26;
    static final int NAVIGATION_STATE_CHANGE = 27;
    static final int PROGRESS = 28;
    static final int END = 29;
    static final int BUFFER = 30;
    static final int VIEWABLE_ITEMS_CHANGE = 31;
    static final int BACK_PRESS = 32;
    static final int SCROLL = 33;
    static final int SCROLL_DRAG_START = 34;
    static final int SCROLL_DRAG_END = 35;
    static final int MOMENTUM_START = 36;
    static final int MOMENTUM_END = 37;
    static final int DIALOG = 38;
    static final int TOUCH = 39;
    static final int SELECTION_CHANGE = 40;
    static final int KEY_PRESS = 41;
    static final int LOAD_END = 42;
    static final int IMAGE_INFO = 43;
    static final int TEXT_LAYOUT = 44;
    static final int CONTENT_SIZE_CHANGE = 45;
    static final int BACK_REQUEST = 46;
    static final int NATIVE_EVENT = 47;
    static final int CLIPBOARD = 48;
    static final int KEY_COMMAND = 49;
    static final int HOVER_IN = 51;
    static final int HOVER_OUT = 52;
    static final int FILE_DRAG_ENTER = 53;
    static final int FILE_DRAG_LEAVE = 54;
    static final int FILE_DROP = 55;
    static final int HOVER_MOVE = 56;
    static final int CONTEXT_MENU = 57;
    static final int COMPOSITION_CHANGE = 58;
    static final int PRESS_OUTSIDE = 59;
    static final int CANCEL = 60;
    static final int POSITION = 61;
    static final int MOUSE_MOVE = 62;

    /**
     * Draws the SVG {@code bytes} for a view {@code maxPx} pixels on its longer side, with its shorter side that long
     * (0: its intrinsic size, {@code density} pixels per point), as premultiplied RGBA, setting {@code size} to the
     * raster's width and height and the SVG's intrinsic size in whole points; {@code maxPx} below 0 only measures (an
     * empty array). {@code intrinsic} receives its unrounded width and height in points. Null when the bytes are
     * not a valid SVG. Any thread.
     */
    static native byte[] svg(byte[] bytes, int maxPx, float density, int[] size, float[] intrinsic);

    /** A host whose apps keep their data in {@code dataDir}, each in a directory named by its id. */
    static native long create(NshView view, float density, boolean trace, String dataDir);

    static native void destroy(long handle);

    /** What loaded apps are granted at most (capabilities JSON; null: no ceiling); false if invalid. */
    static native boolean setGranted(long handle, String capabilities);

    /** VM limits of the apps loaded next (0 keeps a default). */
    static native void setLimits(long handle, int memoryMb, int stackKb, int loadMs, int callMs, int maxNodes);

    /** No VM limits for the apps loaded next: they run on their own. */
    static native void setStandaloneLimits(long handle);

    /** The directory the storage of the apps loaded next is kept in (null: each app's own in the data directory). */
    static native void setStorage(long handle, String dir);

    /** Whether the running app's web views may navigate to {@code url}. */
    static native boolean allowsWebview(long handle, String url);

    /**
     * The URL a media load redirected from {@code base} to {@code location}
     * goes on to, or null if the running app may not load it.
     */
    static native String mediaRedirect(long handle, String base, String location);

    /**
     * The bytes of the in-memory image {@code uri}: a {@code data:} URI's or a
     * {@code blob:} URL's that a view shows, or an {@code Image.getSize}
     * {@code data:} URI's; null when there are none.
     */
    static native byte[] imageBytes(long handle, String uri);

    /** {@code capabilities}: the app's declaration (JSON), {@code app}: its {@code app.json} ({@code id}); null for none. */
    static native void load(long handle, byte[] code, boolean bytecode, String sourceMap, String capabilities, String app);

    static native boolean connectDevServer(long handle, String url);

    /** Runs the Rust app this library is (defined by the app's `nsh::main!`; absent from the JS runtime's). */
    static native void runRust(long handle);

    /** A trust store kept in {@code dir}; 0 if its state cannot be read. */
    static native long trustCreate(String dir);

    static native boolean trustAddRoot(long trust, String publicKey);

    static native boolean trustUpdateRevocations(long trust, String json);

    static native void trustDestroy(long trust);

    /**
     * Verifies and unpacks a signed package (any thread): a package handle and
     * {@code info} = id, name, version, build; or 0 and {@code info[0]} = why.
     */
    static native long trustOpenPackage(long trust, String path, String[] info);

    /** Runs a package from {@link #trustOpenPackage}, taking it. */
    static native void runPackage(long handle, long pkg);

    static native void packageFree(long pkg);

    /** A native component emitted {@code name} with {@code json} data (null: none). */
    static native void componentEvent(long handle, int id, String name, String json);

    /** A native component's own size (dp). */
    static native void componentSize(long handle, int id, float width, float height);

    /** Root size in dp. */
    static native void resize(long handle, float width, float height);

    /**
     * values: safe area top, right, bottom, left, keyboard height, keyboard
     * animation duration (s), font scale, 1 if the language is right to left.
     * locale: the default locale's language tag (`Intl`'s default).
     */
    static native void environment(long handle, float[] values, boolean dark, String locale);

    static native void event(long handle, int id, int kind, String text, int count, float x, float y);

    /**
     * {@code onHoverMove} of view {@code id}: the pointer at ({@code x}, {@code y}) in it and at
     * ({@code pageX}, {@code pageY}) in its root view (dp); {@code modifiers}: 1 Ctrl, 2 Alt,
     * 4 Shift, 8 Meta.
     */
    static native void hover(long handle, int id, float x, float y, float pageX, float pageY, int modifiers);

    /**
     * An event with an object payload ({@code Double}, {@code Boolean} or
     * {@code String} values), or a single value when {@code keys} is null.
     */
    static native void eventMap(long handle, int id, int kind, String[] keys, Object[] values);

    /** Applies the animation items of batches up to {@code seq}; returns bound values. */
    static native float[] afterBatch(long handle, long seq);

    static native float[] tick(long handle, long frameNanos);

    /**
     * A scroll view's offset, after applying {@code applied} of the scrolls
     * the core asked for. Returns bound values when a scroll value changed,
     * else null.
     */
    static native float[] scroll(long handle, int id, float x, float y, int applied);

    /** A zoomable scroll view's zoom scale, reported before its offset. */
    static native void zoom(long handle, int id, float scale);

    /**
     * Touches for the JS responder system. phase: 0 start, 1 move, 2 end,
     * 3 cancel. Per finger: id, target node, path length (in {@code paths},
     * innermost first), pageX, pageY, locationX, locationY (in
     * {@code coords}, dp) and whether it changed. {@code time}: ms.
     */
    static native void touch(long handle, int phase, int[] ids, int[] targets, int[] pathLengths, int[] paths,
            float[] coords, boolean[] changed, long time, int activated);

    static native boolean hasPan(long handle, int id);

    /** state: 0 began, 1 moved, 2 ended, 3 cancelled. */
    static native float[] pan(long handle, int id, int state, float tx, float ty, float vx, float vy);

    /** A pinch (scale) or two-finger rotation (degrees); state as for pan. */
    static native float[] transformGesture(long handle, int id, boolean rotation, int state, float amount, float velocity);

    /** The window of modal {@code id} is {@code width} x {@code height} dp. */
    static native void rootSize(long handle, int id, float width, float height, float top, float right, float bottom, float left);

    /** A display frame began after {@code NshView.requestJsFrame}. */
    static native void frame(long handle);

    /** Interaction states of view {@code id} the core tracks ({@code Mounter.STATE_*}). */
    static native void states(long handle, int id, int states);
}
