# Quire GPUI patch

This is the published `gpui` 0.2.2 crate source, vendored with its Apache-2.0
license and original package metadata. The only source change is in
`src/platform/windows/events.rs`: restored windows use zero client-area insets
and explicit native resize hit targets on all four edges and corners. The
maximized-window inset logic is unchanged to prevent painting across monitor
bounds.

The patch lets Quire's client-painted gradient and surface reach the visible
window edge instead of exposing the DWM resize frame as desktop-colored pixels.
Recheck this patch when updating GPUI; it targets the Windows hidden-titlebar
path in GPUI 0.2.2.
