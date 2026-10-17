<script lang="ts">
  interface Props {
    values: number[];
    secondary?: number[];
    width?: number;
    height?: number;
    points?: number;
  }
  let { values, secondary = [], width = 120, height = 28, points = 120 }: Props = $props();

  function path(data: number[], max: number, close: boolean): string {
    if (data.length < 2) return '';
    const step = width / (points - 1);
    const offset = (points - data.length) * step;
    let d = '';
    data.forEach((v, i) => {
      const x = offset + i * step;
      const y = height - (v / max) * (height - 2) - 1;
      d += `${i === 0 ? 'M' : 'L'}${x.toFixed(1)},${y.toFixed(1)}`;
    });
    if (close) d += `L${width},${height}L${offset},${height}Z`;
    return d;
  }

  let max = $derived(Math.max(1, ...values, ...secondary));
</script>

<svg {width} {height} viewBox="0 0 {width} {height}" class="overflow-visible">
  <path d={path(values, max, true)} fill="var(--color-accent)" fill-opacity="0.15" />
  <path d={path(values, max, false)} fill="none" stroke="var(--color-accent)" stroke-width="1.5" />
  {#if secondary.some((v) => v > 0)}
    <path d={path(secondary, max, false)} fill="none" stroke="var(--color-seed)" stroke-width="1.2" stroke-dasharray="3 2" />
  {/if}
</svg>
