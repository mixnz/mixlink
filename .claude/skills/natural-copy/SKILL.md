---
name: natural-copy
description: Use whenever writing or changing any text a MixLink user will read — UI labels, buttons, dialogs, error and status messages, tray menus, notifications, onboarding, installer screens, release notes, the public README — in English or Vietnamese. Makes copy read like a person on the team wrote it, not an AI. Use it even for a single button label or one error string.
---

# Natural copy

People who spot "AI-written" text stop trusting the product. The tells are predictable, so they can be
avoided. Write like a careful person on the team who knows the product and respects the reader's time.

## Who reads it
Gamers on Windows, mid-session or about to start one. They want the next action, not a pitch.
Write for someone glancing at the screen for two seconds.

## Core rules
- **Say the plain thing.** "Connect", not "Initiate connection". "Can't reach the host", not
  "Unable to establish a connection to the remote host".
- **Messages = what happened + what to do.** "Host stopped responding. Check it's still on, then try again."
  Skip the part that blames or apologises.
- **Buttons are verbs naming the result.** "Pair", "Stop streaming", "Try again". Not "OK" when a better verb exists,
  never "Submit".
- **Sentence case** for everything (titles, buttons, menus). Title Case Looks Like A Template.
- **Short.** Cut every word that does not change the meaning. One idea per sentence.
- **Concrete over abstract.** Numbers, names, the actual device: "Lost 12% of packets", "Pair with DESKTOP-7Q".
- **Same thing, same word**, every screen. Use the glossary below; do not add synonyms for variety.
- **Calm tone.** No exclamation marks except real success the user waited for, and rarely then. No emoji in UI.
- **Write each language natively.** Vietnamese copy is written as Vietnamese, not translated word by word from
  English, and the reverse. Keys stay the same; sentences may differ in shape.

## AI tells to remove

### English
| Tell | Write instead |
|---|---|
| "seamless", "effortless", "unlock", "empower", "elevate", "leverage", "robust", "cutting-edge" | the actual benefit, or nothing |
| "Oops!", "Uh-oh", "Whoops" | state the problem |
| "Please note that…", "It's important to note…", "Simply…", "Just…" | the fact itself |
| "Something went wrong" with no detail | what failed + next step |
| Rule-of-three lists ("fast, smooth, and reliable") | the one claim that is true and specific |
| Em-dash chains and colons everywhere | two short sentences |
| "Successfully connected!" | "Connected" |
| "Are you sure you want to…?" | name the consequence: "Unpair DESKTOP-7Q? You'll need the PIN again." |
| Hedging ("may", "might", "could potentially") when the app knows | say it |
| "In order to" / "utilize" / "via" in UI | "to" / "use" / "with" |

### Tiếng Việt
| Dấu hiệu | Viết thay bằng |
|---|---|
| "Vui lòng…" mở đầu mọi câu | bỏ, hoặc chỉ dùng một lần khi thật sự nhờ người dùng |
| "Thực hiện", "Tiến hành", "Tiến hành kết nối" | động từ thật: "Kết nối" |
| "Đã xảy ra lỗi" trống không | nói lỗi gì + cần làm gì |
| "Bạn có chắc chắn muốn…?" | nói hậu quả: "Bỏ ghép nối DESKTOP-7Q? Lần sau phải nhập lại mã PIN." |
| Câu bị động dịch từ tiếng Anh: "Kết nối đã được thiết lập thành công" | "Đã kết nối" |
| "trải nghiệm liền mạch", "mượt mà vượt trội", "tối ưu hóa" | lợi ích cụ thể, hoặc bỏ |
| Trộn tiếng Anh tuỳ tiện ("Click vào button Connect") | dùng từ trong glossary, nhất quán |
| Xưng hô lẫn lộn ("quý khách", "bạn", "người dùng") | luôn "bạn", hoặc không xưng |
| Dấu chấm than, "Tuyệt vời!" | câu bình thường |
| Viết Hoa Mỗi Chữ | chỉ viết hoa chữ đầu câu và tên riêng |

## Glossary (starting point — extend, never contradict)
| Concept | English | Tiếng Việt |
|---|---|---|
| machine running the game | host | máy host |
| machine playing remotely | client | máy chơi |
| start a session | Connect | Kết nối |
| end a session | Disconnect / Stop streaming | Ngắt kết nối / Dừng stream |
| first-time trust | Pair, pairing | Ghép nối |
| pairing code | PIN | mã PIN |
| delay | latency | độ trễ |
| stream window | stream | stream |

New term → add a row here in the same change. Keep gamer words gamers actually use ("stream", "FPS", "ping").

## Format rules
- Errors: no stack traces, codes only when the user can act on them ("exit code 3" is fine in a crash notice).
- Placeholders are named (`{device}`, `{code}`), never positional, so each language can reorder.
- No string concatenation to build sentences; one key per full sentence.
- Plurals through the i18n layer, not `count + " devices"`.
- Numbers with units and a space: "12 ms", "120 fps".

## Before you finish
1. Read every string aloud. If you would not say it to a friend sitting next to you, rewrite it.
2. Scan for the tells above in both languages.
3. Check each term against the glossary.
4. Both `en` and `vi` exist for every key once the language task has landed (ADR-0003); English is the default.
