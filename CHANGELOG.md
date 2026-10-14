# Changelog

## Unreleased

- Gantt: `tickInterval` (`1day`, `2week`, `1month`, ... as d3's `every`;
  over 1000 ticks falls back to automatic ones) and the today line
  (`todayMarker off` or CSS `stroke`, `stroke-width`, `opacity`). In a
  browser today is local time, elsewhere UTC.
- Git graphs: cherry-pick commits carry Mermaid's cherry marker.

## 0.1.0

First release: Mermaid layout and painting, for egui 0.36.
