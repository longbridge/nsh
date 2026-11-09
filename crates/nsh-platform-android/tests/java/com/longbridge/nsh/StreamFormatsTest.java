package com.longbridge.nsh;

/**
 * Coverage for what a live stream accepts: the GPU buffers it draws and
 * the sound it plays. Plain JDK, no Android APIs, so it runs on a developer
 * machine or in CI without a device:
 *
 * <pre>
 * javac -d target/java-tests \
 *   crates/nsh-platform-android/java/com/longbridge/nsh/StreamFormats.java \
 *   crates/nsh-platform-android/tests/java/com/longbridge/nsh/StreamFormatsTest.java
 * java -cp target/java-tests com.longbridge.nsh.StreamFormatsTest
 * </pre>
 */
public final class StreamFormatsTest {
    private static final long GPU = StreamFormats.USAGE_GPU_SAMPLED_IMAGE;
    private static final long CPU_READ_OFTEN = 3;

    public static void main(String[] args) {
        check("an RGBA buffer the GPU samples draws on API 29",
                StreamFormats.bufferProblem(StreamFormats.RGBA_8888, GPU, 1, 1280, 720, 29) == null);
        check("an RGBX buffer the GPU samples draws on API 34",
                StreamFormats.bufferProblem(StreamFormats.RGBX_8888, GPU, 1, 1280, 720, 34) == null);
        check("a buffer the GPU does not sample is refused",
                StreamFormats.bufferProblem(StreamFormats.RGBA_8888, CPU_READ_OFTEN, 1, 64, 64, 34) != null);
        check("a YUV buffer is refused",
                StreamFormats.bufferProblem(0x23, GPU, 1, 64, 64, 34) != null);
        check("a buffer of two layers is refused",
                StreamFormats.bufferProblem(StreamFormats.RGBA_8888, GPU, 2, 64, 64, 34) != null);
        check("an empty buffer is refused",
                StreamFormats.bufferProblem(StreamFormats.RGBA_8888, GPU, 1, 0, 64, 34) != null);
        check("before API 29 a buffer the CPU cannot read is refused",
                StreamFormats.bufferProblem(StreamFormats.RGBA_8888, GPU, 1, 64, 64, 28) != null);
        check("before API 29 a buffer the CPU reads is copied",
                StreamFormats.bufferProblem(StreamFormats.RGBA_8888, GPU | CPU_READ_OFTEN, 1, 64, 64, 26) == null);

        check("stereo at 48 kHz plays", StreamFormats.pcmProblem(960, 2, 48000) == null);
        check("8 channels at 192 kHz play", StreamFormats.pcmProblem(16, 8, 192000) == null);
        check("mono at 8 kHz plays", StreamFormats.pcmProblem(0, 1, 8000) == null);
        check("no channels is refused", StreamFormats.pcmProblem(4, 0, 48000) != null);
        check("9 channels are refused", StreamFormats.pcmProblem(9, 9, 48000) != null);
        check("a rate under 8 kHz is refused", StreamFormats.pcmProblem(2, 1, 7999) != null);
        check("a rate over 192 kHz is refused", StreamFormats.pcmProblem(2, 1, 192001) != null);
        check("a partial frame is refused", StreamFormats.pcmProblem(3, 2, 48000) != null);

        check("a track holds 200 ms at 48 kHz", StreamFormats.trackFrames(48000, 1024) == 9600);
        check("a track holds the device's minimum when it is more",
                StreamFormats.trackFrames(8000, 4096) == 4096);
        System.out.println("StreamFormatsTest: ok");
    }

    private static void check(String what, boolean ok) {
        if (!ok) throw new AssertionError(what);
    }
}
