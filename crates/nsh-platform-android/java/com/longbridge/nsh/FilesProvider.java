package com.longbridge.nsh;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.content.Context;
import android.database.Cursor;
import android.database.MatrixCursor;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;
import android.webkit.MimeTypeMap;
import java.io.File;
import java.io.FileNotFoundException;
import java.io.IOException;

/**
 * Hands the app's own files to other apps as {@code content:} URIs: the
 * system's viewer reads a file of the app's private storage through {@link
 * #openFile}, which no {@code file:} URI would let it. {@link
 * SystemUi#openPath} builds the URIs ({@link #uriFor}). The authority is the
 * app's package plus {@code .files}, which the packaged apps' manifests
 * declare. No androidx: the columns of {@link #query} are the standard
 * {@link OpenableColumns}, and the grant the viewer needs comes from the
 * manifest's {@code grantUriPermissions}.
 */
public final class FilesProvider extends ContentProvider {
    /** The {@code content:} URI the file at {@code path} opens as. */
    public static Uri uriFor(Context context, File file) {
        return new Uri.Builder()
                .scheme("content")
                .authority(context.getPackageName() + ".files")
                .appendPath(file.getAbsolutePath())
                .build();
    }

    /** The MIME type of the file at {@code path}, its extension's, else
     * {@code application/octet-stream}. */
    public static String typeOf(Context context, File file) {
        String name = file.getName();
        int dot = name.lastIndexOf('.');
        String extension = dot < 0 ? "" : name.substring(dot + 1);
        String mime = MimeTypeMap.getSingleton()
                .getMimeTypeFromExtension(extension.toLowerCase());
        return mime != null ? mime : "application/octet-stream";
    }

    @Override
    public boolean onCreate() {
        return true;
    }

    @Override
    public Cursor query(Uri uri, String[] projection, String selection,
            String[] selectionArgs, String sortOrder) {
        File file = new File(uri.getLastPathSegment());
        String[] columns = projection != null ? projection : new String[] {
                OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE,
        };
        MatrixCursor cursor = new MatrixCursor(columns);
        Object[] row = new Object[columns.length];
        for (int i = 0; i < columns.length; i++) {
            if (OpenableColumns.DISPLAY_NAME.equals(columns[i])) {
                row[i] = file.getName();
            } else if (OpenableColumns.SIZE.equals(columns[i])) {
                row[i] = file.exists() ? file.length() : 0L;
            }
        }
        cursor.addRow(row);
        return cursor;
    }

    @Override
    public String getType(Uri uri) {
        Context context = getContext();
        return typeOf(context, new File(uri.getLastPathSegment()));
    }

    @Override
    public ParcelFileDescriptor openFile(Uri uri, String mode)
            throws FileNotFoundException {
        File file = new File(uri.getLastPathSegment());
        if (!file.exists()) throw new FileNotFoundException(uri.toString());
        return ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY);
    }

    @Override
    public Uri insert(Uri uri, ContentValues values) {
        throw new UnsupportedOperationException();
    }

    @Override
    public int delete(Uri uri, String selection, String[] selectionArgs) {
        throw new UnsupportedOperationException();
    }

    @Override
    public int update(Uri uri, ContentValues values, String selection,
            String[] selectionArgs) {
        throw new UnsupportedOperationException();
    }
}
