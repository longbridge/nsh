package com.longbridge.nsh;

import android.hardware.HardwareBuffer;
import java.nio.ByteBuffer;

/**
 * A live stream the app registered ({@link NshView#registerStream}) for
 * {@code <Video srcObject={HostStream.fromHost(name)}>} in JS. The app
 * pushes its frames from any thread, as BGRA bytes ({@link #pushBgra}, as
 * the iOS, macOS, Windows, Linux and GPUI SDKs take them) or as GPU buffers
 * ({@link #pushHardwareBuffer}, with no copy), and its sound as PCM samples
 * ({@link #pushPcm}). Every video naming the stream draws the frames, fit by
 * its {@code objectFit}: the newest wins, {@code paused} keeps the last one,
 * the first is {@code onLoad}; and plays the sound by its {@code muted},
 * {@code volume} and {@code paused}. While no video shows the stream a push
 * costs nothing.
 *
 * {@link #end} is called on the UI thread; the pushes on any thread.
 */
public final class NshStream {
    private final Mounter mounter;
    final Mounter.Stream stream;

    NshStream(Mounter mounter, Mounter.Stream stream) {
        this.mounter = mounter;
        this.stream = stream;
    }

    /**
     * The name JS uses.
     *
     * @return the name
     */
    public String name() {
        return stream.name;
    }

    /**
     * Shows a frame in the videos naming the stream: {@code width} ×
     * {@code height} pixels of BGRA (8 bits each, opaque or premultiplied),
     * {@code stride} bytes per row (at least {@code width * 4}), read from
     * {@code bytes}'s position on, and copied. Any thread. A frame that
     * comes before the previous one was drawn replaces it (a live stream
     * shows the newest). A frame that does not fit its size (or has none)
     * shows nothing, logged once per stream. While no video shows the
     * stream the bytes are not copied.
     *
     * @param bytes the pixels, {@code stride * height} bytes from its position
     * @param width the frame's width in pixels
     * @param height the frame's height in pixels
     * @param stride the bytes per row
     */
    public void pushBgra(ByteBuffer bytes, int width, int height, int stride) {
        stream.pushBgra(this, bytes, width, height, stride);
    }

    /**
     * Shows a frame held in a GPU buffer (a decoder's, a camera's, a call
     * SDK's) in the videos naming the stream, as {@link #pushBgra} does
     * bytes: the newest wins, {@code paused} keeps the last one, the
     * buffer's size is the first frame's {@code onLoad}. Any thread.
     *
     * <p>The buffer is {@link HardwareBuffer#RGBA_8888} or
     * {@link HardwareBuffer#RGBX_8888} (opaque or premultiplied), one layer,
     * with {@link HardwareBuffer#USAGE_GPU_SAMPLED_IMAGE}. On Android 10 (API
     * 29) and later it is drawn as it is, with no copy. Before Android 10 the
     * system cannot draw a buffer directly, so its pixels are read on the CPU
     * and copied, which takes {@link HardwareBuffer#USAGE_CPU_READ_OFTEN} (or
     * {@code _RARELY}) too. A buffer that does not fit these shows nothing,
     * logged once per stream. While no video shows the stream the buffer is
     * not touched.
     *
     * <p>The stream holds a reference to the buffer while it may still draw
     * it, until a newer frame replaces it and that one is drawn: the caller
     * may {@link HardwareBuffer#close} its own object at once, and writes
     * into a pooled buffer again only once two newer frames were pushed
     * (a pool of at least three buffers).
     *
     * @param buffer the frame
     */
    public void pushHardwareBuffer(HardwareBuffer buffer) {
        stream.pushHardwareBuffer(this, buffer);
    }

    /**
     * Plays sound in the videos naming the stream: {@code samples} are
     * interleaved 32-bit floats in -1 to 1, {@code channels} per frame (1 to
     * 8; the array's length a multiple of it), {@code sampleRate} frames a
     * second (8000 to 192000). Any thread; the samples are copied into each
     * video's audio track without blocking (no more than about 200 ms waits
     * in a track: what does not fit is dropped). Sound is not newest-wins:
     * push it as it comes, in order. A video that is {@code paused} or
     * {@code muted} drops it; the others play it at their {@code volume}. A
     * push of the wrong format plays nothing, logged once per stream. While
     * no video shows the stream the samples are not copied.
     *
     * @param samples the samples, interleaved
     * @param channels the channels per frame
     * @param sampleRate the frames a second
     */
    public void pushPcm(float[] samples, int channels, int sampleRate) {
        stream.pushPcm(this, samples, channels, sampleRate);
    }

    /**
     * Ends the stream: the videos naming it show nothing (their sound plays
     * out what was queued and stops) until the name is registered again.
     * Nothing after a later registration of the name.
     */
    public void end() {
        mounter.endStream(this);
    }
}
