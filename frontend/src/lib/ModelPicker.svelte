<script lang="ts">
  import { get } from 'svelte/store';
  import { t } from '$lib/i18n';

  let {
    available = [],
    selected = $bindable([]),
    disabled = false,
    onFetch,
  }: {
    available?: string[];
    selected?: string[];
    disabled?: boolean;
    onFetch?: () => void;
  } = $props();

  let open = $state(false);
  let query = $state('');
  let manual = $state('');

  /** Groups a model id into a coarse capability bucket. */
  function category(m: string): string {
    const s = m.toLowerCase();
    // Checked in order: a name can match several buckets, the most specific
    // capability wins.
    if (s.includes('embed') || s.startsWith('bge')) return 'embedding';
    if (s.includes('tts') || s.includes('audio') || s.includes('speech') || s.includes('whisper')) return 'audio';
    if (s.includes('reason') || s.includes('r1') || s.includes('think') || s.startsWith('o1') || s.startsWith('o3') || s.startsWith('o4')) return 'reasoning';
    if (s.includes('vl') || s.includes('vision')) return 'vision';
    if (s.includes('image') || s.includes('dall') || s.includes('flux') || s.includes('mj')) return 'image';
    return 'text';
  }

  const CATEGORY_ORDER = ['text', 'reasoning', 'vision', 'image', 'audio', 'embedding'];

  // Everything choosable: what the upstream reported plus anything typed by hand.
  let allModels = $derived([...new Set([...available, ...selected])]);

  let filtered = $derived(
    query.trim()
      ? allModels.filter((m) => m.toLowerCase().includes(query.trim().toLowerCase()))
      : allModels
  );

  let grouped = $derived(
    CATEGORY_ORDER.map((cat) => ({
      cat,
      items: filtered.filter((m) => category(m) === cat),
    })).filter((g) => g.items.length > 0)
  );

  function toggle(m: string) {
    selected = selected.includes(m)
      ? selected.filter((x) => x !== m)
      : [...selected, m];
  }

  function addManual() {
    const value = manual.trim();
    if (!value) return;
    if (!selected.includes(value)) selected = [...selected, value];
    manual = '';
  }

  // `$t` only exists in markup; script code reads the store explicitly.
  function label(): string {
    const tr = get(t);
    if (selected.length === 0) return tr('common.noModels');
    if (selected.length === 1) return selected[0];
    return tr('vendors.selectedCount', { count: selected.length });
  }
</script>

<div class="relative">
<div class="flex gap-2">
  <button
    type="button"
    onclick={() => (open = !open)}
    class="flex-1 flex items-center justify-between gap-2 px-3 py-2 bg-canvas border border-line rounded-lg text-sm text-ink hover:bg-hover transition-colors text-left"
  >
    <span class="truncate {selected.length === 0 ? 'text-faint' : ''}">{label()}</span>
    <span class="shrink-0 w-4 h-4 text-muted [&>svg]:w-full [&>svg]:h-full">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>
    </span>
  </button>
  {#if onFetch}
    <button
      type="button"
      onclick={onFetch}
      {disabled}
      title={disabled ? $t('vendors.apiKeyRequired') : ''}
      class="shrink-0 px-3 py-2 border border-line rounded-lg text-sm font-medium text-ink hover:bg-hover disabled:opacity-50 transition-colors whitespace-nowrap"
    >
      {$t('vendors.fetchModels')}
    </button>
  {/if}
</div>

  {#if open}
    <!-- Click-away layer -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="fixed inset-0 z-10" onclick={() => (open = false)}></div>

    <div class="absolute z-20 mt-1 w-full bg-surface border border-line rounded-lg shadow-xl overflow-hidden">
      <div class="p-2 border-b border-line">
        <input
          type="text"
          bind:value={query}
          placeholder={$t('vendors.searchModels')}
          class="w-full px-2.5 py-1.5 bg-canvas border border-line rounded-md text-sm text-ink placeholder:text-faint focus:outline-none focus:ring-1 focus:ring-accent"
        />
      </div>

      <div class="max-h-64 overflow-y-auto p-1">
        {#if grouped.length === 0}
          <p class="px-2 py-6 text-center text-sm text-muted">{$t('vendors.noModels')}</p>
        {:else}
          {#each grouped as group}
            <div class="px-2 pt-2 pb-1 text-xs font-medium text-faint">{$t(`vendors.cat.${group.cat}`)}</div>
            {#each group.items as m}
              <label class="flex items-center gap-2 px-2 py-1 rounded-md hover:bg-hover cursor-pointer">
                <input
                  type="checkbox"
                  checked={selected.includes(m)}
                  onchange={() => toggle(m)}
                  class="rounded border-line bg-surface text-accent focus:ring-accent"
                />
                <span class="text-sm text-ink font-mono truncate">{m}</span>
              </label>
            {/each}
          {/each}
        {/if}
      </div>

      <div class="p-2 border-t border-line flex gap-2">
        <input
          type="text"
          bind:value={manual}
          placeholder={$t('vendors.manualModel')}
          onkeydown={(e) => e.key === 'Enter' && (e.preventDefault(), addManual())}
          class="flex-1 px-2.5 py-1.5 bg-canvas border border-line rounded-md text-sm text-ink placeholder:text-faint focus:outline-none focus:ring-1 focus:ring-accent"
        />
        <button
          type="button"
          onclick={addManual}
          disabled={!manual.trim()}
          class="px-3 py-1.5 border border-line rounded-md text-sm font-medium text-ink hover:bg-hover disabled:opacity-50 transition-colors"
        >
          {$t('vendors.add')}
        </button>
      </div>
    </div>
  {/if}
</div>
