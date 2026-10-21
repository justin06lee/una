#!/usr/bin/env bash
# Download the openly licensed speech sets for the Korean / code-switching work
# (docs/korean-codeswitch.md). No accounts, no GPU: curl, tar and disk only, at idle CPU and
# IO priority so it can run beside a training job. Safe to re-run: finished sets are skipped
# and interrupted downloads resume.
#
#   server/scripts/koen/fetch_public.sh [DEST]          # FLEURS ko/en, HiKE, Zeroth-Korean (~16 GB)
#   EXTRA="musan rirs librispeech100" fetch_public.sh   # + noise, room responses, EN replay (~19 GB more)
#   RATE=20M fetch_public.sh                            # cap the download rate
#
# Every set is CC BY 4.0 except HiKE and rirs (Apache 2.0); DEST/SOURCES.tsv records what came
# from where.
set -euo pipefail

dest=${1:-$HOME/una-koen}
here=$(cd "$(dirname "$0")" && pwd)
extra=${EXTRA:-}
mkdir -p "$dest"

low() {
  if command -v ionice >/dev/null; then nice -n 19 ionice -c 3 "$@"; else nice -n 19 "$@"; fi
}

need_gb=16
for set in $extra; do
  case $set in musan) need_gb=$((need_gb + 11));; rirs) need_gb=$((need_gb + 2));;
    librispeech100) need_gb=$((need_gb + 7));; esac
done
# archives are deleted once unpacked, but both exist while one unpacks
free_gb=$(df -Pk "$dest" | awk 'NR == 2 { print int($4 / 1048576) }')
if (( free_gb < need_gb * 2 )); then
  echo "need about $((need_gb * 2)) GB free in $dest while unpacking, have $free_gb GB" >&2
  exit 1
fi

fleurs=https://huggingface.co/datasets/google/fleurs/resolve/main/data
sources="$dest/SOURCES.tsv"
[[ -f $sources ]] || printf 'set\turl\tlicense\tsha256\tfetched\n' > "$sources"

fetch() {  # fetch <set> <url> <license> <unpack-into or ->
  local set=$1 url=$2 license=$3 into=$4 file done_mark
  file="$dest/.dl/$(basename "$url")"
  done_mark="$dest/.dl/$set.$(basename "$url").done"
  [[ -f $done_mark ]] && return 0
  mkdir -p "$dest/.dl"
  echo "fetching $set: $url"
  low curl -fsSL --retry 5 --retry-delay 10 -C - ${RATE:+--limit-rate "$RATE"} -o "$file" "$url"
  local sum
  if command -v sha256sum >/dev/null; then sum=$(low sha256sum "$file"); else sum=$(low shasum -a 256 "$file"); fi
  sum=${sum%% *}
  if [[ $into != - ]]; then
    mkdir -p "$into"
    low tar -xf "$file" -C "$into"
    rm -f "$file"
  else
    mv "$file" "$(dirname "$file")/../$(basename "$file")"
  fi
  printf '%s\t%s\t%s\t%s\t%s\n' "$set" "$url" "$license" "$sum" "$(date -u +%FT%TZ)" >> "$sources"
  touch "$done_mark"
}

for lang in ko_kr en_us; do
  for split in train dev test; do
    fetch "fleurs-$lang" "$fleurs/$lang/$split.tsv" "CC BY 4.0" -
    mkdir -p "$dest/fleurs/$lang"
    [[ -f $dest/$split.tsv ]] && mv "$dest/$split.tsv" "$dest/fleurs/$lang/$split.tsv"
    fetch "fleurs-$lang" "$fleurs/$lang/audio/$split.tar.gz" "CC BY 4.0" "$dest/fleurs/$lang"
  done
done

# Korean-English code-switched test set (1,121 utterances, 2.2 h, 13 speakers): eval only
fetch hike https://huggingface.co/datasets/thetaone-ai/HiKE/resolve/main/data/test-00000-of-00001.parquet \
  "Apache 2.0" -
if [[ -f $dest/test-00000-of-00001.parquet ]]; then
  mkdir -p "$dest/hike" && mv "$dest/test-00000-of-00001.parquet" "$dest/hike/test.parquet"
fi

# 51.6 h read Korean, 105 speakers but only 3,000 distinct sentences; 1.2 h test, 10 speakers
fetch zeroth https://www.openslr.org/resources/40/zeroth_korean.tar.gz "CC BY 4.0" "$dest/zeroth"

for set in $extra; do
  case $set in
    musan) fetch musan https://www.openslr.org/resources/17/musan.tar.gz "CC BY 4.0" "$dest/musan" ;;
    rirs) fetch rirs https://www.openslr.org/resources/28/rirs_noises.zip "Apache 2.0" - ;;
    librispeech100) fetch librispeech100 https://www.openslr.org/resources/12/train-clean-100.tar.gz \
      "CC BY 4.0" "$dest/librispeech" ;;
    *) echo "unknown set $set" >&2; exit 1 ;;
  esac
done
if [[ -f $dest/rirs_noises.zip && ! -d $dest/RIRS_NOISES ]]; then
  (cd "$dest" && low unzip -q rirs_noises.zip && rm rirs_noises.zip)
fi

# Manifests for koen_eval.py, when a Python with soundfile, rapidfuzz and pyarrow is at hand
# (e.g. PY=~/una/server/.venv/bin/python).
py=${PY:-python3}
if "$py" -c "import rapidfuzz, pyarrow, soundfile" 2>/dev/null; then
  mkdir -p "$dest/manifests"
  for lang in ko_kr:ko en_us:en; do
    for split in train dev test; do
      "$py" -I "$here/koen_data.py" fleurs "$dest/fleurs/${lang%%:*}" "$split" "${lang##*:}" \
        > "$dest/manifests/fleurs-${lang%%:*}-$split.tsv"
    done
  done
  "$py" -I "$here/koen_data.py" hike "$dest/hike/test.parquet" > "$dest/manifests/hike.tsv"
  for split in train test; do
    dir=$(find "$dest/zeroth" -maxdepth 2 -type d -name "${split}_data*" | head -1)
    [[ -n $dir ]] && "$py" -I "$here/koen_data.py" zeroth "$dir" ko > "$dest/manifests/zeroth-$split.tsv"
  done
  wc -l "$dest"/manifests/*.tsv
fi
du -sh "$dest"
