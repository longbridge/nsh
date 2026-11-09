package com.longbridge.nsh;

/**
 * What a live stream ({@link NshStream}) accepts, as plain functions of
 * numbers: no Android APIs, so {@code tests/java/StreamFormatsTest.java}
 * runs them on a developer machine. The constants are
 * {@code android.hardware.HardwareBuffer}'s.
 */
final class StreamFormats {
    /** {@code HardwareBuffer.RGBA_8888}, {@code RGBX_8888}. */
    static final int RGBA_8888 = 1, RGBX_8888 = 2;
    /** {@code HardwareBuffer.USAGE_GPU_SAMPLED_IMAGE}. */
    static final long USAGE_GPU_SAMPLED_IMAGE = 1L << 8;
    /** The CPU read bits of a usage ({@code USAGE_CPU_READ_RARELY} 2, {@code USAGE_CPU_READ_OFTEN} 3). */
    static final long USAGE_CPU_READ_MASK = 0xF;
    /** The most sound a video's audio track queues, in milliseconds: more is dropped. */
    static final int MAX_QUEUED_MS = 200;

    private StreamFormats() {}

    /**
     * Why a {@code HardwareBuffer} cannot be a frame on API level
     * {@code sdk}, or null when it can: 8-bit RGBA or RGBX, one layer, not
     * empty, sampled by the GPU (`Bitmap.wrapHardwareBuffer` from API 29),
     * and readable by the CPU before (its pixels are copied there).
     */
    static String bufferProblem(int format, long usage, int layers, int width, int height, int sdk) {
        if (width <= 0 || height <= 0) return "the size is empty";
        if (format != RGBA_8888 && format != RGBX_8888) return "the format is not RGBA_8888 or RGBX_8888";
        if (layers != 1) return "the buffer has more than one layer";
        if ((usage & USAGE_GPU_SAMPLED_IMAGE) == 0) return "the usage lacks USAGE_GPU_SAMPLED_IMAGE";
        if (sdk < 29 && (usage & USAGE_CPU_READ_MASK) == 0) {
            return "before Android 10 the pixels are copied on the CPU, and the usage lacks USAGE_CPU_READ_OFTEN";
        }
        return null;
    }

    /**
     * Why {@code length} samples of {@code channels} channels at
     * {@code sampleRate} frames a second cannot be played, or null when
     * they can (nsh-core's {@code check_pcm}, the rule every platform keeps).
     */
    static String pcmProblem(int length, int channels, int sampleRate) {
        if (channels < 1 || channels > 8) return "the channels are not 1 to 8";
        if (sampleRate < 8000 || sampleRate > 192000) return "the sample rate is not 8000 to 192000";
        if (length % channels != 0) return "the samples are not a whole number of frames";
        return null;
    }

    /**
     * The frames a video's audio track holds at {@code sampleRate}: about
     * {@link #MAX_QUEUED_MS} of sound, or the device's minimum
     * ({@code minFrames}) when that is more. A non-blocking write drops
     * what does not fit, so a host pushing faster than the device plays
     * never queues more than this.
     */
    static int trackFrames(int sampleRate, int minFrames) {
        return Math.max(minFrames, (int) ((long) sampleRate * MAX_QUEUED_MS / 1000));
    }
}
