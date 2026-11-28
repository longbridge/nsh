package com.longbridge.nsh;

import java.lang.ref.WeakReference;
import java.util.ArrayList;
import java.util.List;

/**
 * Where a selection is on a text laid out over a {@code PrecomputedText}, and that asking leaves nothing behind.
 *
 * <ul>
 *   <li>The caret of a character inside a one-run line is inside the line, not at its far edge (Android 15 answers the
 *       edge from a layout over a {@code PrecomputedText}), and a point on the line hits the character under it.
 *   <li>The layouts, and the {@code Prepared} they came from, are collected once nothing refers to them: laying out,
 *       hit testing and asking for ranges and carets (the geometry layout is built then) pin none of them.
 * </ul>
 *
 * <p>Runs on a device or emulator (the layouts are Android's), as an instrumentation: a bare {@code app_process} has
 * no system fonts, which the zygote loads for an app. The class is not part of the runtime, the Android build
 * compiles only {@code java/}. With the Android SDK ({@code BT} its build-tools, {@code JAR} an {@code android.jar}
 * of API 26 or later) and a debug key:
 *
 * <pre>
 * javac --release 11 -d out/classes -cp $JAR -sourcepath crates/nsh-platform-android/java \
 *   crates/nsh-platform-android/tests/java/com/longbridge/nsh/SelectionGeometryTest.java
 * $BT/d8 --min-api 26 --lib $JAR --output out/dex $(find out/classes -name '*.class')
 * cat &gt; out/AndroidManifest.xml &lt;&lt;'XML'
 * &lt;manifest xmlns:android="http://schemas.android.com/apk/res/android" package="com.longbridge.nsh.geometrytest"&gt;
 *   &lt;uses-sdk android:minSdkVersion="26" android:targetSdkVersion="35"/&gt;
 *   &lt;application android:label="geometrytest"/&gt;
 *   &lt;instrumentation android:name="com.longbridge.nsh.SelectionGeometryTest"
 *       android:targetPackage="com.longbridge.nsh.geometrytest"/&gt;
 * &lt;/manifest&gt;
 * XML
 * $BT/aapt2 link -o out/base.apk -I $JAR --manifest out/AndroidManifest.xml
 * (cd out/dex &amp;&amp; zip ../base.apk classes.dex)
 * $BT/zipalign -f 4 out/base.apk out/aligned.apk
 * $BT/apksigner sign --ks debug.jks --out out/test.apk out/aligned.apk
 * adb install -r -t out/test.apk
 * adb shell am instrument -w com.longbridge.nsh.geometrytest/com.longbridge.nsh.SelectionGeometryTest
 * </pre>
 *
 * <p>It prints a {@code PASS} or {@code FAIL} line for each check, and {@code ok} when all pass. The bug it covers is
 * Android 15's: run it on API 35 or later.
 */
public final class SelectionGeometryTest extends android.app.Instrumentation {
    private static final String TEXT =
            "The quick brown fox jumps over the lazy dog, and then it keeps running far away from here.";
    private static final float DENSITY = 2.5f;

    private final StringBuilder report = new StringBuilder();

    @Override
    public void onCreate(android.os.Bundle arguments) {
        super.onCreate(arguments);
        start();
    }

    @Override
    public void onStart() {
        super.onStart();
        report.append("API ").append(android.os.Build.VERSION.SDK_INT).append('\n');
        int failed = 0;
        failed += run("a caret inside a one-run line is inside the line", SelectionGeometryTest::caretsInsideLine);
        failed += run("layouts and their text are collected", SelectionGeometryTest::collected);
        report.append(failed == 0 ? "ok" : failed + " failed").append('\n');
        android.os.Bundle result = new android.os.Bundle();
        result.putString(android.app.Instrumentation.REPORT_KEY_STREAMRESULT, report.toString());
        finish(failed == 0 ? android.app.Activity.RESULT_OK : android.app.Activity.RESULT_CANCELED, result);
    }

    private interface Check {
        String run() throws Exception;
    }

    private int run(String what, Check check) {
        String problem;
        try {
            problem = check.run();
        } catch (Throwable t) {
            problem = t.toString();
        }
        report.append(problem == null ? "PASS " : "FAIL ").append(what).append(problem == null ? "" : ": " + problem).append('\n');
        return problem == null ? 0 : 1;
    }

    /** One run over all of {@link #TEXT}: the text of a `<Text>` without spans. */
    private static Object prepare() {
        int[] ints = {0, TEXT.length(), 400, 0, 0xff000000, 0, -1, 0, 0, 0, 0};
        float[] floats = {16, 0, 0, 0, 0, 0, 0, 0};
        return TextLayouts.prepare(TEXT, ints, floats, new String[0], TextLayouts.ALIGN_LEFT, 0, 0, DENSITY, 0, 0, 0, 0,
                new float[0]);
    }

    private static String caretsInsideLine() {
        Object layout = TextLayouts.layout(prepare(), 160, 400);
        float[] lines = TextLayouts.lines(layout, 1f);
        if (lines.length < 2 * TextLayouts.LINE_FLOATS) return "expected a wrapped text, got " + lines.length / TextLayouts.LINE_FLOATS + " lines";
        int end = (int) lines[1];
        float width = lines[2] + lines[4];
        float before = -1;
        for (int offset = 0; offset < end - 1; offset++) {
            float[] caret = TextLayouts.caretRect(layout, offset, false, 1f);
            float x = caret[0];
            if (x < before) return "caret " + offset + " at " + x + " is left of caret " + (offset - 1) + " at " + before;
            if (offset > 0 && x >= width - 1) return "caret " + offset + " at " + x + " is the line's end (" + width + ")";
            before = x;
            // The point on a character's middle hits it.
            float[] next = TextLayouts.caretRect(layout, offset + 1, false, 1f);
            long hit = TextLayouts.hitTest(layout, (x + next[0]) / 2 - 0.2f, 1, 1f);
            if (TextLayouts.hitOffset(hit) != offset && TextLayouts.hitOffset(hit) != offset + 1) {
                return "a point on character " + offset + " hit " + TextLayouts.hitOffset(hit);
            }
        }
        float[] word = TextLayouts.rangeRects(layout, 4, 9, 1f);
        if (word.length != 4) return "a word is " + word.length / 4 + " boxes";
        if (word[2] >= width / 2) return "the word \"quick\" is " + word[2] + " wide of a line " + width;
        return null;
    }

    /** Lays out, asks and lets go; returns only what must not outlive it. */
    private static List<WeakReference<Object>> use() {
        List<WeakReference<Object>> out = new ArrayList<>();
        // More widths than a Prepared keeps, one of them asked after others replaced it.
        for (int round = 0; round < 3; round++) {
            Object prepared = prepare();
            out.add(new WeakReference<>(prepared));
            List<Object> layouts = new ArrayList<>();
            for (int width = 100; width <= 300; width += 40) {
                Object layout = TextLayouts.layout(prepared, width, 400);
                layouts.add(layout);
                out.add(new WeakReference<>(layout));
                // The first round only lays out: a text nobody selects.
                if (round == 0) continue;
                TextLayouts.hitTest(layout, 10, 10, 1f);
                TextLayouts.caretRect(layout, 7, false, 1f);
                TextLayouts.rangeRects(layout, 4, 9, 1f);
            }
            if (round == 2) for (Object layout : layouts) TextLayouts.hitTest(layout, 20, 10, 1f);
        }
        return out;
    }

    private static String collected() {
        List<WeakReference<Object>> refs = use();
        int alive = 0;
        for (int attempt = 0; attempt < 30; attempt++) {
            // Allocation pressure, then a collection: a weakly reachable object goes.
            byte[][] ballast = new byte[64][];
            for (int i = 0; i < ballast.length; i++) ballast[i] = new byte[256 * 1024];
            Runtime.getRuntime().gc();
            alive = 0;
            for (WeakReference<Object> ref : refs) if (ref.get() != null) alive++;
            if (alive == 0) return null;
        }
        return alive + " of " + refs.size() + " layouts and texts are still reachable after collecting";
    }
}
