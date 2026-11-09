package com.longbridge.nsh;

import android.media.AudioAttributes;
import android.media.AudioFormat;
import android.media.AudioTrack;

/**
 * One video's sound of a live stream ({@link NshStream#pushPcm}): an
 * {@code AudioTrack} in streaming mode of 32-bit floats, made at the first
 * samples and again when their channels or rate change, written from the
 * pushing thread without blocking it. The track holds about
 * {@link StreamFormats#MAX_QUEUED_MS} of sound and a non-blocking write
 * drops what does not fit, so a host pushing faster than the device plays
 * never builds up a delay.
 *
 * <p>The video sets its {@code paused}, {@code muted} and {@code volume}
 * from the UI thread: paused or muted (or while another app holds the audio
 * focus it lost), the samples are dropped and what is queued is flushed;
 * otherwise the track plays them at the volume. The lock is held only to
 * write (non-blocking) and to swap the track: a track is made and released
 * outside it, so neither thread waits on the other for long.
 */
final class StreamAudio {
    private final Object lock = new Object();
    /** Tells the video the track started or stopped playing (any thread): its audio focus follows. */
    private final Runnable soundingChanged;
    private AudioTrack track;
    private int channels, rate;
    /** The track was told to play; the samples queued since then are heard. */
    private boolean playing;
    private boolean released;
    /** The format a track could not be made for: not tried again until another comes. */
    private int failedChannels, failedRate;
    private AudioAttributes attributes;
    private volatile boolean paused, muted, lost;
    private volatile float volume = 1;
    /** Whether the track plays (read by the video's audio focus, UI thread). */
    private volatile boolean sounding;

    StreamAudio(AudioAttributes attributes, Runnable soundingChanged) {
        this.attributes = attributes;
        this.soundingChanged = soundingChanged;
    }

    /** Whether the stream's sound plays now. */
    boolean sounding() {
        return sounding;
    }

    /** Pushed samples, checked (`StreamFormats.pcmProblem`); the pushing thread. */
    void write(float[] samples, int channels, int sampleRate) {
        if (paused || muted || lost) return;
        AudioTrack t, stale = null;
        synchronized (lock) {
            if (released) return;
            t = track;
            if (t != null && (this.channels != channels || this.rate != sampleRate)) {
                stale = t;
                track = null;
                t = null;
                setSounding(false);
            }
            if (t == null && failedChannels == channels && failedRate == sampleRate) return;
        }
        if (stale != null) stale.release();
        if (t == null) {
            t = make(channels, sampleRate);
            if (t == null) return;
            synchronized (lock) {
                if (released || track != null) {
                    // Released, or another thread made one meanwhile: that one plays.
                    stale = t;
                    t = track;
                } else {
                    track = t;
                    this.channels = channels;
                    this.rate = sampleRate;
                    playing = false;
                    t.setVolume(volume);
                }
            }
            if (stale != null) stale.release();
            if (t == null) return;
        }
        synchronized (lock) {
            if (track != t || paused || muted || lost) return;
            if (!playing) {
                t.play();
                playing = true;
                setSounding(true);
            }
            t.write(samples, 0, samples.length, AudioTrack.WRITE_NON_BLOCKING);
        }
    }

    /** A track for the format, or null (logged) when the device cannot play it. */
    private AudioTrack make(int channels, int sampleRate) {
        int mask = channelMask(channels);
        int frameBytes = channels * 4;
        int min = AudioTrack.getMinBufferSize(sampleRate, mask, AudioFormat.ENCODING_PCM_FLOAT);
        AudioAttributes attrs;
        synchronized (lock) {
            attrs = attributes;
        }
        try {
            if (min <= 0) throw new UnsupportedOperationException("no buffer size for it");
            int frames = StreamFormats.trackFrames(sampleRate, (min + frameBytes - 1) / frameBytes);
            AudioTrack t = new AudioTrack.Builder()
                    .setAudioAttributes(attrs)
                    .setAudioFormat(new AudioFormat.Builder()
                            .setEncoding(AudioFormat.ENCODING_PCM_FLOAT)
                            .setSampleRate(sampleRate)
                            .setChannelMask(mask)
                            .build())
                    .setBufferSizeInBytes(frames * frameBytes)
                    .setTransferMode(AudioTrack.MODE_STREAM)
                    .build();
            // The buffer may be larger than asked: hold it to the frames.
            t.setBufferSizeInFrames(frames);
            return t;
        } catch (UnsupportedOperationException | IllegalArgumentException e) {
            synchronized (lock) {
                failedChannels = channels;
                failedRate = sampleRate;
            }
            android.util.Log.w("nsh", "a stream's sound of " + channels + " channels at " + sampleRate
                    + " Hz cannot play on this device: " + e.getMessage());
            return null;
        }
    }

    /** The speaker positions of `channels` (1 to 8), as the platforms' layouts order them. */
    private static int channelMask(int channels) {
        switch (channels) {
            case 1: return AudioFormat.CHANNEL_OUT_MONO;
            case 2: return AudioFormat.CHANNEL_OUT_STEREO;
            case 3: return AudioFormat.CHANNEL_OUT_STEREO | AudioFormat.CHANNEL_OUT_FRONT_CENTER;
            case 4: return AudioFormat.CHANNEL_OUT_QUAD;
            case 5: return AudioFormat.CHANNEL_OUT_QUAD | AudioFormat.CHANNEL_OUT_FRONT_CENTER;
            case 6: return AudioFormat.CHANNEL_OUT_5POINT1;
            case 7: return AudioFormat.CHANNEL_OUT_5POINT1 | AudioFormat.CHANNEL_OUT_BACK_CENTER;
            default: return AudioFormat.CHANNEL_OUT_7POINT1_SURROUND;
        }
    }

    /**
     * The video's props (UI thread): paused or muted stops the sound at
     * once (what is queued is flushed); the volume is the track's gain.
     */
    void set(boolean paused, boolean muted, float volume) {
        this.paused = paused;
        this.muted = muted;
        this.volume = Math.max(0, Math.min(1, volume));
        synchronized (lock) {
            if (track != null) track.setVolume(this.volume);
            if (paused || muted) silence();
        }
    }

    /** Another app took the audio focus (true), or gave it back (false). UI thread. */
    void setLost(boolean lost) {
        this.lost = lost;
        if (!lost) return;
        synchronized (lock) {
            silence();
        }
    }

    /** The video's audio session changed (UI thread): the next samples make a track with these attributes. */
    void setAttributes(AudioAttributes attributes) {
        AudioTrack stale;
        synchronized (lock) {
            this.attributes = attributes;
            stale = track;
            track = null;
            failedChannels = 0;
            failedRate = 0;
            setSounding(false);
        }
        if (stale != null) stale.release();
    }

    /** Stops the track now, dropping what is queued; under `lock`. */
    private void silence() {
        if (track == null || !playing) return;
        track.pause();
        track.flush();
        playing = false;
        setSounding(false);
    }

    /**
     * The registration ended (UI thread): what is queued plays out (a
     * stopped streaming track drains first), nothing more until samples
     * come again.
     */
    void end() {
        synchronized (lock) {
            if (track == null || !playing) return;
            track.stop();
            playing = false;
            setSounding(false);
        }
    }

    /** The video goes, or names another stream (UI thread): the track is released, nothing plays again. */
    void release() {
        AudioTrack stale;
        synchronized (lock) {
            released = true;
            stale = track;
            track = null;
            setSounding(false);
        }
        if (stale != null) stale.release();
    }

    private void setSounding(boolean value) {
        if (sounding == value) return;
        sounding = value;
        soundingChanged.run();
    }
}
