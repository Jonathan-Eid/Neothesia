package com.github.polymeilex.neothesia

import android.app.NativeActivity
import android.content.Intent
import android.net.Uri
import android.provider.OpenableColumns
import java.io.File
import java.io.FileOutputStream

/**
 * A plain `android.app.NativeActivity` can't receive `onActivityResult`
 * (the file picker's result), and cargo-apk has no way to bundle a custom
 * Activity subclass at all - hence this small Gradle-built shell around the
 * same native code, just to add that one hook.
 */
class MainActivity : NativeActivity() {
    companion object {
        init {
            System.loadLibrary("neothesia")
        }

        private const val PICK_FILE_REQUEST = 1001
    }

    /** Implemented in `neothesia/src/android.rs`. */
    external fun nativeOnFilePicked(path: String?)

    /** Called from Rust (`android::pick_file`) via JNI. */
    fun openFilePicker() {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "*/*"
        }
        startActivityForResult(intent, PICK_FILE_REQUEST)
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != PICK_FILE_REQUEST) return

        val uri: Uri? = data?.data
        if (uri == null) {
            nativeOnFilePicked(null)
            return
        }

        try {
            val name = queryDisplayName(uri) ?: "picked_file"
            val outFile = File(cacheDir, name)
            contentResolver.openInputStream(uri)?.use { input ->
                FileOutputStream(outFile).use { output -> input.copyTo(output) }
            }
            nativeOnFilePicked(outFile.absolutePath)
        } catch (e: Exception) {
            android.util.Log.e("Neothesia", "Failed to copy picked file", e)
            nativeOnFilePicked(null)
        }
    }

    private fun queryDisplayName(uri: Uri): String? {
        contentResolver.query(uri, null, null, null, null)?.use { cursor ->
            val idx = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
            if (idx >= 0 && cursor.moveToFirst()) {
                return cursor.getString(idx)
            }
        }
        return null
    }
}
