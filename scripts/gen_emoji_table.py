"""Erzeugt src-tauri/core/src/backend/mattermost/emoji_table.rs aus iamcal/emoji-data.

Aufruf: curl -sfLo /tmp/emoji.json https://raw.githubusercontent.com/iamcal/emoji-data/v15.1.2/emoji.json
        python3 -I scripts/gen_emoji_table.py /tmp/emoji.json src-tauri/core/src/backend/mattermost/emoji_table.rs
"""
import json, sys
d = json.load(open(sys.argv[1]))
d.sort(key=lambda e: e.get("sort_order", 0))
out = ["//! Generiert aus iamcal/emoji-data v15.1.2 (MIT), Kurznamen wie in Mattermost/Slack.",
       "//! Nicht von Hand ändern; Erzeugung: `scripts/gen_emoji_table.py`.",
       "",
       "/// (Name, Emoji) in Datensatz-Reihenfolge; der erste Name je Emoji ist der bevorzugte.",
       "pub const TABLE: &[(&str, &str)] = &["]
for e in d:
    uni = "".join(chr(int(c, 16)) for c in e["unified"].split("-"))
    for n in e["short_names"]:
        out.append(f"    ({json.dumps(n)}, {json.dumps(uni, ensure_ascii=False)}),")
out.append("];")
open(sys.argv[2], "w").write("\n".join(out) + "\n")
