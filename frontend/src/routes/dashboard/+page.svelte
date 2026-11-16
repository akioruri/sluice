<script lang="ts">
  import { api } from '$lib/stores';
  import { t } from '$lib/i18n';
  import { icons } from '$lib/icons';
  import LineChart from '$lib/LineChart.svelte';
  import type { Series } from '$lib/LineChart.svelte';
  import type { StatsSummary, DailyStats } from '$lib/admin-api';

  let stats = $state<StatsSummary | null>(null);
  let daily = $state<DailyStats | null>(null);
  let activeKeys = $state(0);
  let loading = $state(true);
  let error = $state('');

  $effect(() => {
    loadStats();
  });

  async function loadStats() {
    try {
      const [summary, series, keys] = await Promise.all([
        api.getStats(),
        api.getDailyStats(),
        api.getTokens(),
      ]);
      stats = summary;
      daily = series;
      // Retired keys are filtered out server-side, so this is the active count.
      activeKeys = keys.length;
    } catch (e) {
      error = e instanceof Error ? e.message : $t('stats.loadFailed');
    } finally {
      loading = false;
    }
  }

  /** Pivots flat (day, series, count) rows into one Series per name. */
  function toSeries(rows: { day: string; series: string; requests: number }[]): Series[] {
    const byName = new Map<string, { day: string; value: number }[]>();
    for (const row of rows) {
      const list = byName.get(row.series) ?? [];
      list.push({ day: row.day, value: row.requests });
      byName.set(row.series, list);
    }
    return [...byName.entries()].map(([name, points]) => ({ name, points }));
  }
</script>

<div class="max-w-5xl">
  <div class="mb-8">
    <h1 class="text-2xl font-semibold text-ink">{$t('dashboard.title')}</h1>
    <p class="mt-2 text-sm text-muted">{$t('dashboard.subtitle')}</p>
  </div>

  {#if loading}
    <div class="text-sm text-muted">{$t('common.loading')}</div>
  {:else if error}
    <div class="flex items-start gap-2 bg-danger/10 border border-danger/30 text-danger px-4 py-3 rounded-lg text-sm">
      <span class="w-4 h-4 mt-0.5 shrink-0 [&>svg]:w-full [&>svg]:h-full">{@html icons.alert}</span>
      {error}
    </div>
  {:else if stats}
    <div class="grid grid-cols-1 gap-4 sm:grid-cols-3">
      {#each [{ label: $t('stats.totalRequests'), value: stats.total_requests.toLocaleString() }, { label: $t('stats.activeVendors'), value: stats.by_provider.length }, { label: $t('stats.activeKeys'), value: activeKeys }] as card}
        <div class="bg-surface border border-line rounded-xl p-5">
          <dt class="text-sm text-muted">{card.label}</dt>
          <dd class="mt-2 text-3xl font-semibold text-ink">{card.value}</dd>
        </div>
      {/each}
    </div>

    <div class="mt-6 bg-surface border border-line rounded-xl p-5">
      <h3 class="text-sm font-medium text-ink mb-4">{$t('dashboard.dailyByVendor')}</h3>
      {#if daily && daily.by_provider.length > 0}
        <LineChart series={toSeries(daily.by_provider)} />
      {:else}
        <p class="text-sm text-muted">{$t('dashboard.noChartData')}</p>
      {/if}
    </div>

    <div class="mt-6 bg-surface border border-line rounded-xl p-5">
      <h3 class="text-sm font-medium text-ink mb-4">{$t('dashboard.dailyByKey')}</h3>
      {#if daily && daily.by_token.length > 0}
        <LineChart series={toSeries(daily.by_token)} />
      {:else}
        <p class="text-sm text-muted">{$t('dashboard.noChartData')}</p>
      {/if}
    </div>

    <div class="mt-6 bg-surface border border-line rounded-xl overflow-hidden">
      <div class="px-5 py-4 border-b border-line">
        <h3 class="text-sm font-medium text-ink">{$t('dashboard.byVendor')}</h3>
      </div>
      {#if stats.by_provider.length === 0}
        <p class="px-5 py-8 text-center text-sm text-muted">{$t('dashboard.noRequests')}</p>
      {:else}
        <table class="w-full table-fixed">
          <thead>
            <tr class="border-b border-line">
              <th class="px-5 py-3 text-left text-xs font-medium text-muted truncate">{$t('stats.vendor')}</th>
              <th class="px-5 py-3 text-left text-xs font-medium text-muted truncate">{$t('stats.requests')}</th>
              <th class="px-5 py-3 text-left text-xs font-medium text-muted truncate">{$t('stats.firstSeen')}</th>
              <th class="px-5 py-3 text-left text-xs font-medium text-muted truncate">{$t('stats.lastSeen')}</th>
            </tr>
          </thead>
          <tbody>
            {#each stats.by_provider as provider}
              <tr class="border-b border-subtle last:border-0 hover:bg-hover transition-colors">
                <td class="px-5 py-3.5 text-sm text-ink font-medium truncate">{provider.provider_name}</td>
                <td class="px-5 py-3.5 text-sm text-muted">{provider.count.toLocaleString()}</td>
                <td class="px-5 py-3.5 text-sm text-muted truncate">{new Date(provider.first_seen).toLocaleString()}</td>
                <td class="px-5 py-3.5 text-sm text-muted truncate">{new Date(provider.last_seen).toLocaleString()}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>
  {/if}
</div>
