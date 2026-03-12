/**
 * test-comp.tsx — minimal smoke-test composition.
 *
 * Renders a progress bar animating from 0% → 100% over durationInFrames.
 * Uses Verdant palette: bg #0a0a0a, teal #7ec8c8, text #e0e0e0.
 * Props-based (no Remotion hooks).
 */

import React from 'react';
import type { FrameProps } from './types.js';

export default function TestComp({
  frame,
  durationInFrames,
  width,
  height,
}: FrameProps): React.ReactElement {
  const progress = durationInFrames > 1 ? frame / (durationInFrames - 1) : 1;
  const pct = Math.round(progress * 100);
  const barWidth = Math.round(progress * (width - 80));

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        width,
        height,
        background: '#0a0a0a',
        fontFamily: 'Inter, sans-serif',
        color: '#e0e0e0',
      }}
    >
      {/* Title */}
      <div
        style={{
          display: 'flex',
          fontSize: 32,
          fontWeight: 700,
          marginBottom: 40,
          color: '#7ec8c8',
          letterSpacing: 2,
        }}
      >
        pi-kit render pipeline
      </div>

      {/* Bar track */}
      <div
        style={{
          display: 'flex',
          width: width - 80,
          height: 24,
          background: '#1a1a1a',
          borderRadius: 12,
          overflow: 'hidden',
        }}
      >
        {/* Bar fill */}
        <div
          style={{
            display: 'flex',
            width: barWidth,
            height: 24,
            background: '#7ec8c8',
            borderRadius: 12,
          }}
        />
      </div>

      {/* Percentage label */}
      <div
        style={{
          display: 'flex',
          marginTop: 20,
          fontSize: 24,
          color: '#e0e0e0',
          fontVariantNumeric: 'tabular-nums',
        }}
      >
        {pct}%
      </div>
    </div>
  );
}
