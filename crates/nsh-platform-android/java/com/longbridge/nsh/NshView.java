package com.longbridge.nsh;

import android.app.Activity;
import android.content.Context;
import android.content.ContextWrapper;
import android.content.res.Configuration;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;
import android.text.SpannableString;
import android.text.Spanned;
import android.text.style.ForegroundColorSpan;
import android.util.TypedValue;
import android.view.Choreographer;
import android.view.Gravity;
import android.view.View;
import android.view.Window;
import android.view.WindowInsets;
import android.view.WindowManager;
import android.widget.FrameLayout;
import android.widget.HorizontalScrollView;
import android.widget.ScrollView;
import android.widget.TextView;
import java.io.File;
import java.io.FileInputStream;
import java.io.InputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;

/**
 * A view running a nsh app. Add it anywhere in a view hierarchy, then load
 * a bundle ({@link #loadBundleFile}, {@link #loadAssetBundle}), a signed
 * package ({@link NshTrust#openPackage}, {@link #runPackage}) or, in
 * development builds, connect to a `nsh dev` server. Call {@link #destroy}
 * when done.
 */
public class NshView extends FrameLayout {
    /** Receives fatal app errors on the UI thread. */
    public interface ErrorListener {
        /**
         * Called for each fatal error.
         *
         * @param message what went wrong
         */
        void onError(String message);
    }

    long handle;
    private final float density;
    private final Box root;
    private final Mounter mounter;
    private final TouchTracker touches;

    @Override
    public boolean dispatchTouchEvent(android.view.MotionEvent e) {
        boolean tracking = touches.onTouch(e);
        // A tracked finger went down: hear the rest of the gesture even if
        // no view takes it.
        return super.dispatchTouchEvent(e) || tracking;
    }
    private final Handler main = new Handler(Looper.getMainLooper());
    private ErrorListener errorListener;
    /** Where `asset:/…` files come from: an http base, a directory, or APK assets. */
    private volatile String assetHttp;
    private volatile File assetDir;
    private volatile String assetApk;
    private TextView status;
    private View errorView;

    /**
     * Creates an empty view; load an app into it.
     *
     * @param context the activity
     */
    public NshView(Context context) {
        this(context, false);
    }

    /**
     * Creates an empty view; load an app into it.
     *
     * @param context the activity
     * @param trace log per-commit timings
     */
    public NshView(Context context, boolean trace) {
        super(context);
        density = getResources().getDisplayMetrics().density;
        TextLayouts.cacheDir = context.getCacheDir();
        mounter = new Mounter(this, new Images(this::readAsset));
        root = mounter.root;
        addView(root, new LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT));
        touches = new TouchTracker(mounter, this);
        mounter.touchTrackers.add(touches);
        setClipChildren(false);
        // Holds focus when no input does (see InputBox.blur).
        setFocusable(true);
        setFocusableInTouchMode(true);
        // Not a visible focus target.
        setDefaultFocusHighlightEnabled(false);
        setDescendantFocusability(FOCUS_BEFORE_DESCENDANTS);
        // Apps scroll inputs clear of the keyboard themselves: when the app
        // sets no soft input mode, this makes the system resize (and report
        // the keyboard as insets) instead of panning the window.
        setScrollContainer(true);
        handle = Native.create(this, density, trace);
    }

    /**
     * Receives fatal app errors (load failures, uncaught JS exceptions) on the
     * UI thread. They are always logged.
     *
     * @param listener the listener, or null for none
     */
    public void setErrorListener(ErrorListener listener) {
        errorListener = listener;
    }

    /**
     * What the apps this view loads are granted at most, as JSON in the
     * form of {@code nsh.json}'s {@code capabilities} ({@code null}: what
     * each app declares). Applies from the next load.
     *
     * @param capabilities the grant, or null
     * @return false (and ignored) when the grant is invalid
     */
    public boolean setGrantedCapabilities(String capabilities) {
        return Native.setGranted(handle, capabilities);
    }

    /**
     * The VM limits of the apps this view loads next: heap (MB), stack (KB),
     * the time budget of loading and of every other call into JS (ms), and
     * the node quota. 0 keeps a default.
     *
     * @param memoryMb JS heap, in MB
     * @param stackKb interpreter stack, in KB
     * @param loadMs time to evaluate the bundle, in ms
     * @param callMs time of any other call into JS, in ms
     * @param maxNodes nodes the app may keep
     */
    public void setLimits(int memoryMb, int stackKb, int loadMs, int callMs, int maxNodes) {
        Native.setLimits(handle, memoryMb, stackKb, loadMs, callMs, maxNodes);
    }

    /**
     * Runs a JS bundle written by `nsh build` (`bundle.js`); assets come from
     * `assets/` next to it. Bytecode only loads from the APK's own assets
     * ({@link #loadAssetBundle}) or a signed package ({@link #runPackage}):
     * files can be replaced, and QuickJS does not validate bytecode.
     *
     * @param file the bundle
     * @return false (reported to the error listener) if it cannot be read
     */
    public boolean loadBundleFile(File file) {
        if (file.getName().endsWith(".nshc")) {
            reportError("bytecode only loads from the APK's assets or a signed package (.nshpkg); load bundle.js");
            return false;
        }
        try {
            byte[] code = Net.readAll(new FileInputStream(file));
            String map = null;
            File mapFile = new File(file.getParentFile(), "bundle.js.map");
            if (mapFile.exists()) map = new String(Net.readAll(new FileInputStream(mapFile)), StandardCharsets.UTF_8);
            File declared = new File(file.getParentFile(), "capabilities.json");
            String capabilities = declared.exists()
                    ? new String(Net.readAll(new FileInputStream(declared)), StandardCharsets.UTF_8) : null;
            assetHttp = null;
            assetApk = null;
            assetDir = new File(file.getParentFile(), "assets");
            Native.load(handle, code, false, map, capabilities);
            return true;
        } catch (Exception e) {
            reportError("cannot read bundle " + file + ": " + e);
            return false;
        }
    }

    /**
     * Runs {@code dir/bundle.nshc} from the APK's assets; app assets come
     * from {@code dir/assets/}.
     *
     * @param dir the directory in the APK's assets
     * @return false (reported to the error listener) if it cannot be read
     */
    public boolean loadAssetBundle(String dir) {
        try {
            byte[] code = Net.readAll(getContext().getAssets().open(dir + "/bundle.nshc"));
            String capabilities = null;
            try {
                capabilities = new String(Net.readAll(getContext().getAssets().open(dir + "/capabilities.json")),
                        StandardCharsets.UTF_8);
            } catch (java.io.IOException none) {
                // The app declares no capabilities.
            }
            assetHttp = null;
            assetDir = null;
            assetApk = dir + "/assets/";
            Native.load(handle, code, true, null, capabilities);
            return true;
        } catch (Exception e) {
            reportError("cannot read the app bundle: " + e);
            return false;
        }
    }

    /**
     * Runs the Rust app compiled into {@code libnsh.so} (an app built with
     * {@code nsh package --rust}); its assets come from {@code dir/assets/}
     * in the APK.
     *
     * @param dir the directory in the APK's assets
     */
    public void runRustApp(String dir) {
        assetHttp = null;
        assetDir = null;
        assetApk = dir + "/assets/";
        Native.runRust(handle);
    }

    /**
     * Runs a verified package ({@link NshTrust#openPackage}), replacing the
     * running app. Takes the package: it cannot run again.
     *
     * @param pkg the package
     */
    public void runPackage(NshPackage pkg) {
        Native.runPackage(handle, pkg.take());
    }

    /** Called by {@code Native.runPackage} before the package starts. */
    void usePackageAssets(String dir) {
        assetHttp = null;
        assetApk = null;
        assetDir = new File(dir);
    }

    /**
     * Registers native component {@code name} ({@code nativeComponent(name, ...)}
     * in JS): {@code factory} makes one for every element created from now on.
     *
     * @param name the name JS uses
     * @param factory makes the components (UI thread)
     */
    public void registerComponent(String name, NshComponent.Factory factory) {
        mounter.components.put(name, factory);
    }

    /**
     * Development: loads from a `nsh dev` server and reloads on every change.
     *
     * @param url the server, {@code ws://host:port/ws}
     * @return false without development support (the release AAR)
     */
    public boolean connectDevServer(String url) {
        // ws://host:port/ws -> assets at http://host:port/assets/
        if (url.startsWith("ws://")) {
            String rest = url.substring(5);
            int slash = rest.indexOf('/');
            assetHttp = "http://" + (slash >= 0 ? rest.substring(0, slash) : rest) + "/assets/";
        }
        assetDir = null;
        assetApk = null;
        return Native.connectDevServer(handle, url);
    }

    /** Stops the app. The view can no longer be used. */
    public void destroy() {
        if (handle != 0) {
            Native.destroy(handle);
            handle = 0;
        }
    }

    @Override
    protected void onSizeChanged(int w, int h, int oldw, int oldh) {
        super.onSizeChanged(w, h, oldw, oldh);
        // The environment follows in onLayout.
        if (handle != 0 && w > 0 && h > 0) Native.resize(handle, w / density, h / density);
    }

    // --- Environment ---------------------------------------------------------

    private float keyboard;

    @Override
    public WindowInsets onApplyWindowInsets(WindowInsets insets) {
        publishEnvironment(insets);
        return super.onApplyWindowInsets(insets);
    }

    @Override
    protected void onConfigurationChanged(Configuration config) {
        super.onConfigurationChanged(config);
        publishEnvironment(getRootWindowInsets());
    }

    private static Window windowOf(Context context) {
        while (context instanceof ContextWrapper) {
            if (context instanceof Activity) return ((Activity) context).getWindow();
            context = ((ContextWrapper) context).getBaseContext();
        }
        return null;
    }

    /** Whether the app asked the system to pan the window over the keyboard. */
    private boolean systemPans() {
        Window window = windowOf(getContext());
        return window != null
            && (window.getAttributes().softInputMode & WindowManager.LayoutParams.SOFT_INPUT_MASK_ADJUST) == WindowManager.LayoutParams.SOFT_INPUT_ADJUST_PAN;
    }

    @Override
    protected void onLayout(boolean changed, int left, int top, int right, int bottom) {
        super.onLayout(changed, left, top, right, bottom);
        // Insets depend on where the view sits in the window.
        if (changed) publishEnvironment(getRootWindowInsets());
    }

    /**
     * Publishes the safe area and keyboard as the parts of the system bars
     * and the keyboard that overlap this view, like UIKit's safe area: an
     * app embedded below a toolbar gets no top inset.
     */
    private void publishEnvironment(WindowInsets insets) {
        if (handle == 0) return;
        float top = 0, right = 0, bottom = 0, left = 0, ime = 0;
        if (insets != null) {
            int barsTop, barsRight, barsBottom, barsLeft, keyboardBottom = 0;
            if (Build.VERSION.SDK_INT >= 30) {
                android.graphics.Insets bars = insets.getInsets(WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout());
                barsTop = bars.top;
                barsRight = bars.right;
                barsBottom = bars.bottom;
                barsLeft = bars.left;
                keyboardBottom = insets.getInsets(WindowInsets.Type.ime()).bottom;
            } else {
                barsTop = insets.getSystemWindowInsetTop();
                barsRight = insets.getSystemWindowInsetRight();
                barsBottom = insets.getSystemWindowInsetBottom();
                barsLeft = insets.getSystemWindowInsetLeft();
            }
            View window = getRootView();
            int[] at = new int[2];
            getLocationInWindow(at);
            int gapLeft = at[0], gapTop = at[1];
            int gapRight = window.getWidth() - at[0] - getWidth();
            int gapBottom = window.getHeight() - at[1] - getHeight();
            top = Math.max(0, barsTop - gapTop);
            right = Math.max(0, barsRight - gapRight);
            bottom = Math.max(0, barsBottom - gapBottom);
            left = Math.max(0, barsLeft - gapLeft);
            // How far the keyboard reaches into the view (like UIKit: the
            // bottom safe area under it included).
            ime = Math.min(getHeight(), Math.max(0, keyboardBottom - gapBottom));
        }
        if (ime != keyboard) {
            keyboard = ime;
            // Nothing to do when the system pans the window instead.
            int[] at = new int[2];
            getLocationOnScreen(at);
            mounter.keyboard(ime > 0 && !systemPans()
                    ? at[1] + getHeight() - Math.round(ime) : Integer.MAX_VALUE);
        }
        Configuration c = getResources().getConfiguration();
        boolean dark = (c.uiMode & Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES;
        safe = new float[] {top / density, right / density, bottom / density, left / density};
        Native.environment(handle, new float[] {
            top / density, right / density, bottom / density, left / density, ime / density, 0.25f, c.fontScale,
            android.text.TextUtils.getLayoutDirectionFromLocale(java.util.Locale.getDefault()) == LAYOUT_DIRECTION_RTL ? 1 : 0,
            // "Remove animations" sets the animator scale to 0.
            android.provider.Settings.Global.getFloat(getContext().getContentResolver(),
                    android.provider.Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0 ? 1 : 0,
            density,
            accessibility().isTouchExplorationEnabled() ? 1 : 0,
            Build.VERSION.SDK_INT,
            (c.uiMode & Configuration.UI_MODE_TYPE_MASK) == Configuration.UI_MODE_TYPE_TELEVISION ? 3
                    : c.smallestScreenWidthDp >= 600 ? 1 : 0,
        }, dark, java.util.Locale.getDefault().toLanguageTag());
    }

    private android.view.accessibility.AccessibilityManager accessibility() {
        return (android.view.accessibility.AccessibilityManager) getContext().getSystemService(Context.ACCESSIBILITY_SERVICE);
    }

    /** TalkBack turning on or off: `AccessibilityInfo` and its events. */
    private final android.view.accessibility.AccessibilityManager.TouchExplorationStateChangeListener screenReader =
            enabled -> publishEnvironment(getRootWindowInsets());

    /** The safe-area insets last published (dp: top, right, bottom, left). */
    private float[] safe = new float[4];

    float safeTop() {
        return safe[0];
    }

    float safeRight() {
        return safe[1];
    }

    float safeBottom() {
        return safe[2];
    }

    float safeLeft() {
        return safe[3];
    }

    // --- Back button -------------------------------------------------------------

    private Object backCallback;
    private boolean backRegistered;
    private boolean exiting;

    /**
     * Handles the back button for the app: closes modals, pops screens, runs
     * JS `BackHandler` listeners. Activities on Android 12 and older call it
     * from {@code onBackPressed} and fall back to the default when it
     * returns false; newer versions get the press through a callback.
     *
     * @return whether the app handled the press
     */
    public boolean handleBack() {
        if (exiting || handle == 0) return false;
        return mounter.handleBack();
    }

    /** Whether the app wants back presses (registers the callback). */
    void setBackHandling(boolean wanted) {
        if (Build.VERSION.SDK_INT < 33) return;
        android.window.OnBackInvokedDispatcher dispatcher = findOnBackInvokedDispatcher();
        if (dispatcher == null) return;
        if (backCallback == null) backCallback = BackCallbacks.create(this, mounter);
        android.window.OnBackInvokedCallback callback = (android.window.OnBackInvokedCallback) backCallback;
        if (wanted && !backRegistered) {
            dispatcher.registerOnBackInvokedCallback(android.window.OnBackInvokedDispatcher.PRIORITY_DEFAULT, callback);
            backRegistered = true;
        } else if (!wanted && backRegistered) {
            dispatcher.unregisterOnBackInvokedCallback(callback);
            backRegistered = false;
        }
    }

    @Override
    protected void onAttachedToWindow() {
        super.onAttachedToWindow();
        accessibility().addTouchExplorationStateChangeListener(screenReader);
        // The dispatcher exists once attached.
        if (handle != 0) mounter.updateBack();
    }

    @Override
    protected void onDetachedFromWindow() {
        super.onDetachedFromWindow();
        accessibility().removeTouchExplorationStateChangeListener(screenReader);
    }

    /** `BackHandler.exitApp()`: what the back button does without the app. */
    void exitApp() {
        android.app.Activity activity = activity(getContext());
        if (activity == null) return;
        exiting = true;
        if (Build.VERSION.SDK_INT >= 33) setBackHandling(false);
        activity.onBackPressed();
        exiting = false;
        if (handle != 0) mounter.updateBack();
    }

    private static android.app.Activity activity(Context context) {
        while (context instanceof android.content.ContextWrapper) {
            if (context instanceof android.app.Activity) return (android.app.Activity) context;
            context = ((android.content.ContextWrapper) context).getBaseContext();
        }
        return null;
    }

    // --- Animation frames ------------------------------------------------------

    private boolean ticking;
    private final Choreographer.FrameCallback frame = this::onFrame;

    private void onFrame(long nanos) {
        ticking = false;
        if (handle != 0) mounter.outputs(Native.tick(handle, nanos));
    }

    /** Keeps frames coming while native values animate. */
    void requestFrames() {
        if (!ticking) {
            ticking = true;
            Choreographer.getInstance().postFrameCallback(frame);
        }
    }

    // --- Called by libnsh (any thread) ----------------------------------------------

    private final Choreographer.FrameCallback jsFrame = nanos -> {
        if (handle != 0) Native.frame(handle);
    };

    /** JS waits for a display frame (`requestAnimationFrame`). */
    void requestJsFrame() {
        main.post(() -> Choreographer.getInstance().postFrameCallback(jsFrame));
    }

    void submit(byte[] ops, Object[] objects, long seq) {
        main.post(() -> {
            if (handle != 0) mounter.apply(ops, objects, seq);
        });
    }

    void reportError(String message) {
        android.util.Log.e("nsh", message);
        main.post(() -> {
            if (errorListener != null) errorListener.onError(message);
        });
    }

    /** Bytes of an `asset:/…` URI, or null. Blocking: engine and image threads. */
    byte[] readAsset(String uri) {
        String path = uri.startsWith("asset:/") ? uri.substring(7) : uri;
        try {
            String http = assetHttp;
            File dir = assetDir;
            String apk = assetApk;
            if (http != null) {
                HttpURLConnection c = (HttpURLConnection) new URL(http + path).openConnection();
                try {
                    return Net.readAll(c.getInputStream());
                } finally {
                    c.disconnect();
                }
            }
            if (dir != null) {
                try (InputStream in = new FileInputStream(new File(dir, path))) {
                    return Net.readAll(in);
                }
            }
            if (apk != null) return Net.readAll(getContext().getAssets().open(apk + path));
        } catch (Exception e) {
            android.util.Log.w("nsh", "cannot read " + uri + ": " + e);
        }
        return null;
    }

    /**
     * Points a media player at a URL or an {@code asset:/…} URI (streamed from
     * the dev server, read from the asset directory or from the APK).
     */
    void setMediaSource(android.media.MediaPlayer player, String uri) throws java.io.IOException {
        if (!uri.startsWith("asset:/")) {
            player.setDataSource(getContext(), android.net.Uri.parse(uri));
            return;
        }
        String path = uri.substring(7);
        String http = assetHttp;
        File dir = assetDir;
        String apk = assetApk;
        if (http != null) {
            player.setDataSource(getContext(), android.net.Uri.parse(http + path));
        } else if (dir != null) {
            player.setDataSource(new File(dir, path).getPath());
        } else if (apk != null) {
            try (android.content.res.AssetFileDescriptor fd = getContext().getAssets().openFd(apk + path)) {
                player.setDataSource(fd);
            } catch (java.io.FileNotFoundException compressed) {
                // Compressed in the APK: players need a seekable file.
                File copy = new File(getContext().getCacheDir(), "nsh-media/" + path);
                if (!copy.exists()) {
                    copy.getParentFile().mkdirs();
                    File tmp = new File(copy.getPath() + ".tmp");
                    try (InputStream in = getContext().getAssets().open(apk + path);
                            java.io.OutputStream out = new java.io.FileOutputStream(tmp)) {
                        byte[] buf = new byte[64 * 1024];
                        for (int n; (n = in.read(buf)) > 0; ) out.write(buf, 0, n);
                    }
                    tmp.renameTo(copy);
                }
                player.setDataSource(copy.getPath());
            }
        }
    }

    /** How far the system bars reach into this view from the top (0 when embedded below them). */
    private int topInset() {
        WindowInsets insets = getRootWindowInsets();
        if (insets == null) return 0;
        int[] at = new int[2];
        getLocationInWindow(at);
        return Math.max(0, insets.getSystemWindowInsetTop() - at[1]);
    }

    /** Development: a status line over the app, or none. */
    void devStatus(String text) {
        main.post(() -> {
            if (text == null) {
                if (status != null) removeView(status);
                status = null;
                return;
            }
            if (status == null) {
                status = new TextView(getContext());
                status.setTextColor(Color.WHITE);
                status.setBackgroundColor(0xd91a1a1a);
                status.setGravity(Gravity.CENTER);
                status.setTextSize(TypedValue.COMPLEX_UNIT_SP, 13);
                LayoutParams lp = new LayoutParams(LayoutParams.MATCH_PARENT, Math.round(32 * density));
                lp.gravity = Gravity.TOP;
                status.setTranslationY(topInset());
                addView(status, lp);
            }
            status.setText(text);
            status.bringToFront();
        });
    }

    /** Development: the error screen (tap to dismiss; its Copy button copies the error), or
     * none. With {@code prose} the message wraps to the screen; otherwise its lines are kept
     * (code frames, stack traces) and it scrolls sideways. */
    void devError(String title, String message, boolean prose) {
        main.post(() -> {
            if (errorView != null) removeView(errorView);
            errorView = null;
            if (title == null) return;
            TextView text = new TextView(getContext());
            text.setTextColor(Color.WHITE);
            text.setTypeface(Typeface.MONOSPACE);
            text.setTextSize(TypedValue.COMPLEX_UNIT_SP, 12);
            // The title alone says it is an error, in a soft red.
            SpannableString all = new SpannableString(title + "\n\n" + message + "\n\n(tap to dismiss)");
            all.setSpan(new ForegroundColorSpan(0xffff6b6b), 0, title.length(), Spanned.SPAN_EXCLUSIVE_EXCLUSIVE);
            text.setText(all);
            int pad = Math.round(16 * density);
            text.setPadding(pad, topInset() + pad, pad, pad);
            ScrollView scroll = new ScrollView(getContext());
            if (prose) {
                scroll.addView(text);
            } else {
                HorizontalScrollView sideways = new HorizontalScrollView(getContext());
                sideways.addView(text);
                scroll.addView(sideways);
            }
            TextView copy = new TextView(getContext());
            copy.setText("Copy");
            copy.setTextColor(Color.WHITE);
            copy.setTextSize(TypedValue.COMPLEX_UNIT_SP, 13);
            copy.setTypeface(Typeface.DEFAULT_BOLD);
            copy.setGravity(Gravity.CENTER);
            GradientDrawable pill = new GradientDrawable();
            pill.setColor(0x2effffff);
            pill.setCornerRadius(6 * density);
            copy.setBackground(pill);
            String error = title + "\n\n" + message;
            copy.setOnClickListener(v -> {
                SystemUi.setClipboard(getContext(), error);
                copy.setText("Copied");
            });
            LayoutParams at = new LayoutParams(Math.round(76 * density), Math.round(30 * density));
            at.gravity = Gravity.TOP | Gravity.END;
            at.topMargin = topInset() + Math.round(12 * density);
            at.rightMargin = pad;
            FrameLayout view = new FrameLayout(getContext());
            view.setBackgroundColor(0xfa1e1f22);
            view.addView(scroll, new LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT));
            view.addView(copy, at);
            // Clear of the status and navigation bars, also when the screen shows
            // before the window has its insets.
            view.setOnApplyWindowInsetsListener((v, insets) -> {
                int[] where = new int[2];
                getLocationInWindow(where);
                int top = Math.max(0, insets.getSystemWindowInsetTop() - where[1]);
                text.setPadding(pad, top + pad, pad, insets.getSystemWindowInsetBottom() + pad);
                at.topMargin = top + Math.round(12 * density);
                copy.setLayoutParams(at);
                return insets;
            });
            scroll.setOnClickListener(v -> devError(null, null, false));
            text.setOnClickListener(v -> devError(null, null, false));
            errorView = view;
            addView(view, new LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT));
            view.requestApplyInsets();
        });
    }
}
