#!/bin/zsh
set -eu
cd "${0:A:h}"
task_release_app="$PWD/tauri-app/src-tauri/target/release/bundle/macos/OWON Scope.app"
task_debug_app="$PWD/tauri-app/src-tauri/target/debug/bundle/macos/OWON Scope.app"
if [[ -d "$task_release_app" ]]; then
  open "$task_release_app"
elif [[ -d "$task_debug_app" ]]; then
  open "$task_debug_app"
else
  print 'まだビルドされていません。tauri-appで npm install と npm run tauri -- build を実行してください。'
  exit 1
fi
