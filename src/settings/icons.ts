// Inline SVG glyphs (20x20, Fluent-style stroke: currentColor, width 2, round caps).
// spec §7.1 IA order / R4. ponytail: recognizable > pretty — glyphs, not artwork.
// Rendered via innerHTML in main.ts; compile-time constants (no XSS surface).

export const ICONS = {
  // 1. Recognition — microphone (voice→text).
  recognition: `<svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="7" y="2" width="6" height="11" rx="3"/><path d="M4 10a6 6 0 0 0 12 0"/><line x1="10" y1="16" x2="10" y2="19"/><line x1="7" y1="19" x2="13" y2="19"/></svg>`,
  // 2. Microphone — input level meter (device + live level).
  microphone: `<svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="3" y1="10" x2="3" y2="10"/><line x1="6" y1="7" x2="6" y2="13"/><line x1="9" y1="4" x2="9" y2="16"/><line x1="12" y1="7" x2="12" y2="13"/><line x1="15" y1="9" x2="15" y2="11"/><line x1="18" y1="10" x2="18" y2="10"/></svg>`,
  // 3. Text — capital A (font/type).
  text: `<svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 17 L10 3 L16 17"/><line x1="6" y1="12" x2="14" y2="12"/></svg>`,
  // 4. Dictionary — open book.
  dictionary: `<svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 5v12"/><path d="M3 5h6v12H4a1 1 0 0 1-1-1z"/><path d="M17 5h-6v12h5a1 1 0 0 0 1-1z"/></svg>`,
  // Snippets — cue bar + expansion lines (a stored text block).
  snippets: `<svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="3.5" y1="5" x2="3.5" y2="15"/><line x1="7" y1="6" x2="16" y2="6"/><line x1="7" y1="10" x2="14" y2="10"/><line x1="7" y1="14" x2="15" y2="14"/></svg>`,
  // 5. History — clock.
  history: `<svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="10" cy="10" r="7"/><polyline points="10 6 10 10 13 12"/></svg>`,
  // 6. Hotkey — keyboard.
  hotkey: `<svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="5" width="16" height="11" rx="2"/><line x1="6" y1="9" x2="6.01" y2="9"/><line x1="10" y1="9" x2="10.01" y2="9"/><line x1="14" y1="9" x2="14.01" y2="9"/><line x1="7" y1="13" x2="13" y2="13"/></svg>`,
  // 7. Overlay — overlapping windows.
  overlay: `<svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="4" width="12" height="12" rx="1"/><rect x="6" y="8" width="12" height="8" rx="1" fill="var(--bg)"/></svg>`,
  // 8. Updates — download arrow.
  updates: `<svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="10" y1="3" x2="10" y2="13"/><polyline points="6 9 10 13 14 9"/><line x1="4" y1="17" x2="16" y2="17"/></svg>`,
  // 9. About — info i.
  about: `<svg width="20" height="20" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="10" cy="10" r="8"/><line x1="10" y1="10" x2="10" y2="14.5"/><circle cx="10" cy="6.5" r="1" fill="currentColor" stroke="none"/></svg>`,
} as const;
