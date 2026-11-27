package com.longbridge.nsh;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.os.Build;
import android.util.Log;

/**
 * Broadcasts received in Rust ({@code nsh::receiver} of the {@code nsh-platform-android} crate): a receiver registered
 * on the application for one action whose {@link #onReceive} reaches the Rust handler {@code handle} names, so Rust
 * code (an nshm) hears a broadcast without shipping a {@link BroadcastReceiver} subclass of its own, which {@code
 * nsh::proxy} cannot make (it is a class, not an interface). Not exported: it hears the app's own broadcasts (a
 * {@code PendingIntent}'s) and the system's, not other apps'.
 */
final class NativeReceiver extends BroadcastReceiver {
    private static final String TAG = "com.longbridge.nsh.NativeReceiver";
    private final long handle;

    private NativeReceiver(long handle) {
        this.handle = handle;
    }

    /**
     * Registers a receiver of {@code action} whose broadcasts reach handler {@code handle} on the UI thread; {@code
     * null} before any {@code NshView} was made. Any thread.
     */
    static Object register(String action, long handle) {
        Context context = Permissions.application();
        if (context == null) return null;
        NativeReceiver receiver = new NativeReceiver(handle);
        IntentFilter filter = new IntentFilter(action);
        if (Build.VERSION.SDK_INT >= 33) {
            context.registerReceiver(receiver, filter, Context.RECEIVER_NOT_EXPORTED);
        } else {
            context.registerReceiver(receiver, filter);
        }
        return receiver;
    }

    /** Unregisters {@code receiver} ({@link #register}'s). Any thread. */
    static void unregister(Object receiver) {
        Context context = Permissions.application();
        if (context == null) return;
        try {
            context.unregisterReceiver((BroadcastReceiver) receiver);
        } catch (IllegalArgumentException e) {
            Log.w(TAG, "the receiver was not registered", e);
        }
    }

    @Override
    public void onReceive(Context context, Intent intent) {
        receive(handle, intent);
    }

    /** Hands {@code intent} to handler {@code handle}, while Rust keeps it. */
    private static native void receive(long handle, Intent intent);
}
