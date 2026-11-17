#!/bin/sh
# The site's pictures of the kit: every page script under demos/, pictured by the drive in a dark
# and a light theme into public/shots/. The fonts are the terminal's and the system's the pictures
# are drawn in.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(dirname "$here")
fonts=${PENCERE_SHOT_FONTS:-$HOME/Library/Fonts}
cd "$root"
cargo build -q -p pencere-drive --features shot --example shots
target=${CARGO_TARGET_DIR:-$root/target}
"$target/debug/examples/shots" "$here/demos" "$here/public/shots" \
	--terminal "$fonts/JetBrainsMonoNerdFont-Regular.ttf" \
	--bold "$fonts/JetBrainsMonoNerdFont-Bold.ttf" \
	--italic "$fonts/JetBrainsMonoNerdFont-Italic.ttf" \
	--system "$fonts/InterDisplay-Regular.ttf"
