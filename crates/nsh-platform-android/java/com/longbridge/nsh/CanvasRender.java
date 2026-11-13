package com.longbridge.nsh;

import android.graphics.Bitmap;
import android.graphics.Canvas;
import java.io.ByteArrayOutputStream;
import java.util.List;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * `OffscreenCanvas`'s `convertToBlob` (`Command::RenderCanvas`, command 55 of `encode.rs`): the images the drawing
 * names are loaded first (on the UI thread, through {@link Images}), then the drawing is replayed by a
 * {@link CanvasPainter} of its own into a transparent bitmap on a background thread, y down at the requested scale,
 * its animated values at 0, and encoded; the answer is {@link Native#canvasImage} on the UI thread.
 */
final class CanvasRender {
    private CanvasRender() {}

    /** One thread: renders run one after another, so at most one large bitmap is alive at a time. */
    private static final ExecutorService RENDERS = Executors.newSingleThreadExecutor(r -> {
        Thread t = new Thread(r, "nsh-canvas-render");
        t.setDaemon(true);
        return t;
    });

    /** The encodings, `encode.rs`'s numbers of `ImageFormat`. */
    private static final int PNG = 0;

    /**
     * Renders drawing `code` (with its batch's `objects`) into a `width` × `height` pixel image at `scale` pixels
     * per point and answers request `id`. On the UI thread.
     */
    static void render(Mounter mounter, int id, int width, int height, float scale, int format, int[] code,
            Object[] objects) {
        CanvasPainter painter = new CanvasPainter();
        List<String> uris = painter.setDrawing(code, objects);
        float density = mounter.density;
        Runnable job = () -> RENDERS.execute(() -> {
            byte[] bytes = null;
            String error = null;
            try {
                bytes = encode(paint(painter, width, height, scale, density), format);
                if (bytes == null) error = "the image could not be encoded";
            } catch (OutOfMemoryError | RuntimeException e) {
                error = "the canvas could not be drawn: " + e;
            }
            byte[] result = bytes;
            String failure = error;
            mounter.host.post(() -> mounter.canvasImage(id, result, failure));
        });
        // Each image once, failed ones left out (the drawing skips them, as a canvas skips images not loaded).
        java.util.LinkedHashSet<String> wanted = new java.util.LinkedHashSet<>(uris);
        if (wanted.isEmpty()) {
            job.run();
            return;
        }
        int[] waiting = {wanted.size()};
        for (String uri : wanted) {
            // At their natural size, as a canvas on screen loads them.
            Images.Image cached = mounter.images.request(uri, 0, 0, (image, failure) -> {
                if (image != null) {
                    painter.images.put(uri, image);
                } else {
                    android.util.Log.w("nsh", "offscreen canvas image failed: " + failure);
                }
                if (--waiting[0] == 0) job.run();
            });
            if (cached != null) {
                painter.images.put(uri, cached);
                if (--waiting[0] == 0) job.run();
            }
        }
    }

    /** The drawing replayed into a transparent `width` × `height` bitmap, y down, `scale` pixels per point. */
    static Bitmap paint(CanvasPainter painter, int width, int height, float scale, float density) {
        Bitmap bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888);
        painter.paint(new Canvas(bitmap), width, height, scale, density);
        return bitmap;
    }

    /** `bitmap` encoded as `format`, or null; the bitmap is recycled. */
    private static byte[] encode(Bitmap bitmap, int format) {
        try {
            if (format != PNG) return null;
            ByteArrayOutputStream out = new ByteArrayOutputStream();
            // PNG is lossless: the quality is ignored.
            return bitmap.compress(Bitmap.CompressFormat.PNG, 100, out) ? out.toByteArray() : null;
        } finally {
            bitmap.recycle();
        }
    }
}
