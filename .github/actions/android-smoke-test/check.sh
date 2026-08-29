#!/usr/bin/env bash
# reactivecircus/android-emulator-runner runs each line of its `script:`
# input as a *separate* `sh -c` invocation, not as one shell session, so any
# multi-line control flow (if/fi, loops) has to live in its own script file
# invoked as a single line instead.
set -euo pipefail

adb wait-for-device
adb install -r android/app/build/outputs/apk/debug/app-debug.apk
adb logcat -c
adb shell am start -n com.github.polymeilex.neothesia/.MainActivity
sleep 10

if ! adb shell pidof com.github.polymeilex.neothesia > /dev/null; then
  echo "App is not running after launch (crashed or failed to start)"
  adb logcat -d
  exit 1
fi

adb logcat -d > logcat.txt
if grep -qE "FATAL EXCEPTION|SIGSEGV|SIGABRT|signal 6|signal 11" logcat.txt; then
  echo "Crash signature found in logcat:"
  cat logcat.txt
  exit 1
fi

echo "App launched and is still running - smoke test passed"
