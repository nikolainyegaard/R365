# Design

The look is JW365's: dark canvas, a surface-coloured header with brand title and badge, a grid of resource cards, the Microsoft blue accent, one pixel borders. Tokens live in `crates/ui/src/theme.rs` and carry JW365's stylesheet names; views never name a colour.

| Token | Use |
|-------|-----|
| CANVAS | window background |
| SURFACE | header, status bar, dialogs, icon well |
| CARD, CARD_HOVER | resource cards, secondary buttons, text fields |
| BORDER, FOCUS | hairlines; FOCUS on hover and focused fields |
| TEXT, TEXT_SECONDARY, TEXT_MUTED | three levels of text |
| BLUE, BLUE_HOVER, BLUE_PRESSED | primary buttons |
| INFO, ERROR_TEXT | status bar text when something is in progress or failed |
| BADGE_* | (background, text) pairs for type, idle, busy, connected, failed and brand badges |

Shapes: controls 32 px tall with a 6 px radius, cards 220 by 206 with a 12 px radius, dialogs 12 px, the sign-in card 16 px. No shadows; JW365's drop shadows were left out because egui paints them as blurred rects that cost more than they add.

Widgets in `crates/ui/src/widgets.rs`: `button` (Primary, Secondary, Ghost), `badge`, `pill`, `monitor_glyph` (the stand-in for resource icons until icon loading exists), `text` and `strong`.

Motion: none. The only moving parts are egui spinners while something is in progress.

Window: 1050 by 720 at start, 750 by 550 minimum, app id `r365` matching the FreeRDP `/wm-class` so sessions group with the client.
