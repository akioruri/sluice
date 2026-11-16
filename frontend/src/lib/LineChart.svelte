<script lang="ts">
  import { t } from '$lib/i18n';

  export interface Series {
    name: string;
    points: { day: string; value: number }[];
  }

  let { series }: { series: Series[] } = $props();

  const W = 720;
  const H = 260;
  const PAD = { top: 12, right: 16, bottom: 34, left: 44 };

  // A stable colour per series index. Tuned to read on both light and dark.
  const PALETTE = ['#4d6bfe', '#e0574f', '#0f9d58', '#d97706', '#8b5cf6', '#0891b2'];

  let derived = $derived.by(() => {
    const days = [...new Set(series.flatMap((s) => s.points.map((p) => p.day)))].sort();
    const max = Math.max(1, ...series.flatMap((s) => s.points.map((p) => p.value)));

    const innerW = W - PAD.left - PAD.right;
    const innerH = H - PAD.top - PAD.bottom;

    const x = (i: number) =>
      days.length <= 1 ? PAD.left + innerW / 2 : PAD.left + (i / (days.length - 1)) * innerW;
    const y = (v: number) => PAD.top + innerH - (v / max) * innerH;

    // ~6 date labels regardless of how many days are in range.
    const labelStep = Math.max(1, Math.ceil(days.length / 6));

    return {
      days,
      max,
      x,
      y,
      labelStep,
      lines: series.map((s, idx) => {
        const byDay = new Map(s.points.map((p) => [p.day, p.value]));
        const coords = days.map((d, i) => ({ x: x(i), y: y(byDay.get(d) ?? 0) }));
        const path = coords.map((c, i) => `${i === 0 ? 'M' : 'L'}${c.x},${c.y}`).join(' ');
        return { name: s.name, color: PALETTE[idx % PALETTE.length], path, coords };
      }),
    };
  });
</script>

<div class="w-full overflow-x-auto">
  <svg viewBox="0 0 {W} {H}" class="w-full h-auto" role="img">
    {#if derived.days.length === 0}
      <text x={W / 2} y={H / 2} text-anchor="middle" class="fill-muted text-sm">
        {$t('dashboard.noChartData')}
      </text>
    {:else}
      <!-- horizontal gridlines + y labels -->
      {#each [0, 0.25, 0.5, 0.75, 1] as frac}
        {@const gy = PAD.top + (H - PAD.top - PAD.bottom) * (1 - frac)}
        <line x1={PAD.left} y1={gy} x2={W - PAD.right} y2={gy} class="stroke-line" stroke-width="1" />
        <text x={PAD.left - 8} y={gy + 4} text-anchor="end" class="fill-faint text-xs">
          {Math.round(derived.max * frac)}
        </text>
      {/each}

      <!-- x labels -->
      {#each derived.days as day, i}
        {#if i % derived.labelStep === 0}
          <text x={derived.x(i)} y={H - 10} text-anchor="middle" class="fill-faint text-xs">
            {day.slice(5)}
          </text>
        {/if}
      {/each}

      <!-- one line per series -->
      {#each derived.lines as line}
        <path d={line.path} fill="none" stroke={line.color} stroke-width="2" stroke-linejoin="round" />
        {#each line.coords as c}
          <circle cx={c.x} cy={c.y} r="2.5" fill={line.color} />
        {/each}
      {/each}
    {/if}
  </svg>

  {#if derived.days.length > 0}
    <div class="flex flex-wrap gap-x-4 gap-y-1 mt-2 px-1">
      {#each derived.lines as line}
        <span class="flex items-center gap-1.5 text-xs text-muted">
          <span class="inline-block w-3 h-0.5 rounded" style="background:{line.color}"></span>
          {line.name}
        </span>
      {/each}
    </div>
  {/if}
</div>
