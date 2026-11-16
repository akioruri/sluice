<script lang="ts">
  import { api } from '$lib/stores';
  import { t } from '$lib/i18n';
  import { icons } from '$lib/icons';
  import ModelPicker from '$lib/ModelPicker.svelte';
  import type { Vendor, ProviderTestResult } from '$lib/admin-api';

  let vendors = $state<Vendor[]>([]);
  let loading = $state(true);
  let error = $state('');
  let notice = $state('');

  let showForm = $state(false);
  let editingVendor = $state<Vendor | null>(null);
  let deletingName = $state<string | null>(null);

  let name = $state('');
  let upstream = $state('');
  let authHeader = $state('');
  let authFormat = $state('Bearer {key}');
  let apiKey = $state('');
  let formError = $state('');
  let saving = $state(false);

  // Model options for the picker, fetched from the upstream once a key exists.
  let availableModels = $state<string[]>([]);
  let selectedModels = $state<string[]>([]);

  // Reserved names come from the server: they are the ones the router refuses, so
  // a second copy in the browser would eventually disagree with it.
  let reservedNames = $state<string[]>([]);

  // Per-row test results, keyed by vendor name.
  let testResults = $state<Record<string, ProviderTestResult>>({});
  let testingName = $state<string | null>(null);
  let fetchingModels = $state(false);
  let noticeOk = $state(false);

  $effect(() => {
    loadVendors();
    loadConfig();
  });

  async function loadConfig() {
    try {
      const config = await api.getConfig();
      reservedNames = config.reserved_names;
    } catch {
      // Non-fatal: the server still rejects reserved names, so the worst case is
      // a confusing error message instead of a clean inline one.
    }
  }

  async function loadVendors() {
    try {
      vendors = await api.getProviders();
    } catch (e) {
      error = e instanceof Error ? e.message : $t('vendors.loadFailed');
    } finally {
      loading = false;
    }
  }

  /** Tests a saved vendor, probing only the models it is configured for. */
  async function testVendor(v: Vendor) {
    testingName = v.name;
    delete testResults[v.name];
    const models = v.models.split(',').map((m) => m.trim()).filter(Boolean);
    try {
      testResults[v.name] = await api.testProvider(v.name, {
        upstream: v.upstream,
        auth_header: v.auth_header,
        auth_format: v.auth_format,
        models,
      });
    } catch (e) {
      testResults[v.name] = {
        ok: false,
        error: e instanceof Error ? e.message : $t('vendors.test')
      };
    } finally {
      testingName = null;
    }
  }

  /**
   * Fetches the upstream's model list into the picker.
   * Uses the inline form values so an unsaved vendor can be probed too.
   */
  async function fetchModels() {
    if (!upstream.trim()) {
      notice = $t('vendors.required');
      noticeOk = false;
      return;
    }
    if (!editingVendor && !apiKey.trim()) {
      notice = $t('vendors.apiKeyRequired');
      noticeOk = false;
      return;
    }

    fetchingModels = true;
    notice = '';
    noticeOk = false;
    try {
      const result = await api.fetchProviderModels(name.trim() || 'pending', {
        upstream: upstream.trim(),
        auth_header: authHeader.trim(),
        auth_format: authFormat.trim(),
        api_key: apiKey.trim() || undefined,
      });

      if (result.error) {
        availableModels = [];
        notice = result.error;
        return;
      }
      if (result.models.length === 0) {
        availableModels = [];
        notice = $t('vendors.noModels');
        return;
      }

      availableModels = result.models;
      // Keep every current choice. An upstream's model list is not guaranteed to
      // be exhaustive — image or audio models are often served from a separate
      // endpoint — so dropping selections the list omits would silently lose
      // working configuration.
      notice = $t('vendors.fetchedCount', { count: result.models.length });
      noticeOk = true;
    } catch (e) {
      notice = e instanceof Error ? e.message : $t('vendors.fetchFailed');
    } finally {
      fetchingModels = false;
    }
  }

  function toggleModel(m: string) {
    selectedModels = selectedModels.includes(m)
      ? selectedModels.filter((x) => x !== m)
      : [...selectedModels, m];
  }

  /** Compact "N models" label for the table, so wide lists do not blow it up. */
  function modelCount(csv: string): string {
    const n = csv.split(',').map((m) => m.trim()).filter(Boolean).length;
    if (n === 0) return '—';
    return n === 1 ? $t('vendors.modelCountOne') : $t('vendors.modelCount', { count: n });
  }

  function openCreate() {
    editingVendor = null;
    name = '';
    upstream = '';
    authHeader = 'Authorization';
    authFormat = 'Bearer {key}';
    apiKey = '';
    formError = '';
    notice = '';
    noticeOk = false;
    availableModels = [];
    selectedModels = [];
    showForm = true;
  }

  function openEdit(v: Vendor) {
    editingVendor = v;
    name = v.name;
    upstream = v.upstream;
    authHeader = v.auth_header;
    authFormat = v.auth_format;
    apiKey = '';
    formError = '';
    notice = '';
    noticeOk = false;
    // Seed the picker from whatever is already saved, so the current
    // selection is visible even before re-fetching.
    availableModels = v.models
      .split(',')
      .map((m) => m.trim())
      .filter(Boolean);
    selectedModels = [...availableModels];
    showForm = true;
  }

  function closeForm() {
    showForm = false;
    editingVendor = null;
    availableModels = [];
    selectedModels = [];
  }

  async function submit() {
    formError = '';
    if (!name.trim() || !upstream.trim() || !authHeader.trim() || !authFormat.trim()) {
      formError = $t('vendors.required');
      return;
    }
    if (!editingVendor && !apiKey.trim()) {
      formError = $t('vendors.apiKeyRequired');
      return;
    }

    // Reserved names would shadow a gateway route (e.g. /vendors, /api/admin).
    // The list comes from the server so it cannot drift from what is enforced.
    if (!editingVendor && reservedNames.includes(name.trim().toLowerCase())) {
      formError = $t('vendors.reserved', { name: name.trim() });
      return;
    }

    // An empty model list means "no restriction" — that is a valid choice, so it
    // must not block saving. The label in the picker spells out what it means.

    const chosenModels = selectedModels.join(', ');

    saving = true;
    try {
      if (editingVendor) {
        const updates: Record<string, string> = {
          upstream,
          auth_header: authHeader,
          auth_format: authFormat,
          models: chosenModels,
        };
        if (apiKey.trim()) {
          updates.api_key = apiKey.trim();
        }
        await api.updateProvider(editingVendor.name, updates);
      } else {
        await api.createProvider({
          name: name.trim(),
          upstream: upstream.trim(),
          auth_header: authHeader.trim(),
          auth_format: authFormat.trim(),
          models: chosenModels,
          api_key: apiKey.trim()
        });
      }
      closeForm();
      await loadVendors();
    } catch (e) {
      formError = e instanceof Error ? e.message : $t('vendors.saveFailed');
    } finally {
      saving = false;
    }
  }

  async function confirmDelete() {
    if (!deletingName) return;
    try {
      await api.deleteProvider(deletingName);
      deletingName = null;
      await loadVendors();
    } catch (e) {
      error = e instanceof Error ? e.message : $t('vendors.deleteFailed');
      deletingName = null;
    }
  }
</script>

<div class="max-w-5xl">
  <div class="flex items-start justify-between gap-4 mb-8">
    <div>
      <h1 class="text-2xl font-semibold text-ink">{$t('vendors.title')}</h1>
      <p class="mt-2 text-sm text-muted">{$t('vendors.subtitle')}</p>
    </div>
    <button
      onclick={openCreate}
      class="shrink-0 px-4 py-2 rounded-lg text-sm font-medium bg-ink text-canvas hover:opacity-90 transition-opacity"
    >
      {$t('vendors.add')}
    </button>
  </div>

  {#if loading}
    <div class="text-sm text-muted">{$t('common.loading')}</div>
  {:else if error}
    <div class="flex items-start gap-2 bg-danger/10 border border-danger/30 text-danger px-4 py-3 rounded-lg text-sm">
      <span class="w-4 h-4 mt-0.5 shrink-0 [&>svg]:w-full [&>svg]:h-full">{@html icons.alert}</span>
      {error}
    </div>
  {:else if vendors.length === 0}
    <div class="bg-surface border border-line rounded-xl px-5 py-12 text-center text-sm text-muted">{$t('vendors.none')}</div>
  {:else}
    <div class="bg-surface border border-line rounded-xl overflow-hidden">
      <table class="w-full table-fixed">
        <thead>
          <tr class="border-b border-line">
            <th class="px-5 py-3 text-left text-xs font-medium text-muted truncate">{$t('vendors.name')}</th>
            <th class="px-5 py-3 text-left text-xs font-medium text-muted truncate">{$t('vendors.upstream')}</th>
            <th class="px-5 py-3 text-left text-xs font-medium text-muted truncate">{$t('vendors.authHeader')}</th>
            <th class="px-5 py-3 text-left text-xs font-medium text-muted truncate">{$t('vendors.authFormat')}</th>
            <th class="px-5 py-3 text-left text-xs font-medium text-muted">{$t('vendors.models')}</th>
            <th class="px-5 py-3 text-right text-xs font-medium text-muted w-24">{$t('vendors.actions')}</th>
          </tr>
        </thead>
        <tbody>
          {#each vendors as vendor}
            <tr class="border-b border-subtle last:border-0 hover:bg-hover transition-colors">
              <td class="px-5 py-3.5 text-sm text-ink font-medium truncate">{vendor.name}</td>
              <td class="px-5 py-3.5 text-sm text-muted truncate" title={vendor.upstream}>{vendor.upstream}</td>
              <td class="px-5 py-3.5 text-sm text-muted truncate">{vendor.auth_header}</td>
              <td class="px-5 py-3.5 text-sm text-muted font-mono text-xs truncate">{vendor.auth_format}</td>
              <td class="px-5 py-3.5 text-sm text-muted">{modelCount(vendor.models)}</td>
              <td class="px-5 py-3.5 text-right align-top">
                <div class="flex items-center justify-end gap-1">
                  <button
                    onclick={() => openEdit(vendor)}
                    title={$t('common.edit')}
                    aria-label={$t('common.edit')}
                    class="p-1.5 rounded-md text-muted hover:bg-active hover:text-ink transition-colors"
                  >
                    <span class="w-4 h-4 block [&>svg]:w-full [&>svg]:h-full">{@html icons.edit}</span>
                  </button>
                  <button
                    onclick={() => testVendor(vendor)}
                    disabled={testingName === vendor.name}
                    title={$t('vendors.test')}
                    aria-label={$t('vendors.test')}
                    class="p-1.5 rounded-md text-muted hover:bg-active hover:text-ink disabled:opacity-50 transition-colors"
                  >
                    <span class="w-4 h-4 block [&>svg]:w-full [&>svg]:h-full">{@html icons.plug}</span>
                  </button>
                </div>
              </td>
            </tr>
            {#if testResults[vendor.name]}
              {@const r = testResults[vendor.name]}
              <tr class="border-b border-subtle last:border-0">
                <td colspan="6" class="px-5 pb-3 pt-0 text-right">
                  {#if r.results}
                    <div class="flex flex-wrap justify-end gap-x-4 gap-y-1">
                      {#each r.results as item}
                        <span class="text-xs {item.ok ? 'text-success' : 'text-danger'}">
                          {#if item.ok}
                            <span class="font-mono">{item.model}</span> ✓ {item.latency_ms}ms
                          {:else}
                            <span class="font-mono">{item.model}</span> ✗ {item.error}
                          {/if}
                        </span>
                      {/each}
                    </div>
                    {#if r.passed !== r.tested}
                      <p class="text-xs text-danger mt-0.5">
                        {$t('vendors.testPartial', { passed: r.passed ?? 0, total: r.tested ?? 0 })}
                      </p>
                    {/if}
                  {:else if r.ok}
                    <span class="text-xs text-success">
                      {$t('vendors.testOk', { count: r.models ?? 0, ms: r.latency_ms ?? 0 })}
                    </span>
                  {:else}
                    <span class="text-xs text-danger">
                      {$t('vendors.testFailed')}: {r.error}
                    </span>
                  {/if}
                </td>
              </tr>
            {/if}
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>

{#if showForm}
  <div class="fixed inset-0 bg-black/50 backdrop-blur-sm flex items-center justify-center z-50 p-4">
    <div class="bg-surface border border-line rounded-xl shadow-2xl max-w-lg w-full max-h-[90vh] overflow-y-auto">
      <div class="px-6 py-5">
        <h3 class="text-lg font-semibold text-ink mb-5">
          {editingVendor ? $t('vendors.edit') : $t('vendors.add')}
        </h3>
        {#if formError}
          <div class="flex items-start gap-2 bg-danger/10 border border-danger/30 text-danger px-4 py-3 rounded-lg mb-5 text-sm">
            <span class="w-4 h-4 mt-0.5 shrink-0 [&>svg]:w-full [&>svg]:h-full">{@html icons.alert}</span>
            {formError}
          </div>
        {/if}
        <form onsubmit={submit} class="space-y-4">
          <div>
            <label for="name" class="block text-sm font-medium text-ink mb-1.5">{$t('vendors.name')}</label>
            <input
              id="name"
              type="text"
              bind:value={name}
              disabled={!!editingVendor}
              class="block w-full px-3 py-2 bg-canvas border border-line rounded-lg text-sm text-ink placeholder:text-faint focus:outline-none focus:ring-1 focus:ring-accent disabled:opacity-50"
            />
          </div>
          <div>
            <label for="upstream" class="block text-sm font-medium text-ink mb-1.5">{$t('vendors.upstream')}</label>
            <input
              id="upstream"
              type="text"
              bind:value={upstream}
              placeholder="https://api.example.com"
              class="block w-full px-3 py-2 bg-canvas border border-line rounded-lg text-sm text-ink placeholder:text-faint focus:outline-none focus:ring-1 focus:ring-accent"
            />
          </div>
          <div class="grid grid-cols-2 gap-4">
            <div>
              <label for="authHeader" class="block text-sm font-medium text-ink mb-1.5">{$t('vendors.authHeader')}</label>
              <input
                id="authHeader"
                type="text"
                bind:value={authHeader}
                class="block w-full px-3 py-2 bg-canvas border border-line rounded-lg text-sm text-ink placeholder:text-faint focus:outline-none focus:ring-1 focus:ring-accent"
              />
            </div>
            <div>
              <label for="authFormat" class="block text-sm font-medium text-ink mb-1.5">{$t('vendors.authFormat')}</label>
              <input
                id="authFormat"
                type="text"
                bind:value={authFormat}
                class="block w-full px-3 py-2 bg-canvas border border-line rounded-lg text-sm text-ink font-mono placeholder:text-faint focus:outline-none focus:ring-1 focus:ring-accent"
              />
            </div>
          </div>
          <div>
            <label for="apiKey" class="block text-sm font-medium text-ink mb-1.5">
              {editingVendor ? $t('vendors.apiKeyNew') : $t('vendors.apiKey')}
            </label>
            <input
              id="apiKey"
              type="password"
              bind:value={apiKey}
              class="block w-full px-3 py-2 bg-canvas border border-line rounded-lg text-sm text-ink placeholder:text-faint focus:outline-none focus:ring-1 focus:ring-accent"
            />
            {#if !editingVendor && !apiKey.trim()}
              <p class="mt-1.5 text-xs text-faint">{$t('vendors.apiKeyFirst')}</p>
            {/if}
          </div>
          <div>
            <span class="block text-sm font-medium text-ink mb-1.5">{$t('vendors.models')}</span>
            <ModelPicker
              bind:selected={selectedModels}
              available={availableModels}
              disabled={fetchingModels || !apiKey.trim()}
              onFetch={fetchModels}
            />
            <p class="mt-1.5 text-xs text-faint">{$t('vendors.modelsHint')}</p>
            {#if notice}
              <p class="mt-1.5 text-xs {noticeOk ? 'text-success' : 'text-danger'}">{notice}</p>
            {/if}
          </div>
          <div class="flex justify-end gap-3 pt-2">
            <button
              type="button"
              onclick={closeForm}
              class="px-4 py-2 border border-line rounded-lg text-sm font-medium text-ink hover:bg-hover transition-colors"
            >
              {$t('common.cancel')}
            </button>
            <button
              type="submit"
              disabled={saving}
              class="px-4 py-2 rounded-lg text-sm font-medium bg-ink text-canvas hover:opacity-90 disabled:opacity-50 transition-opacity"
            >
              {saving ? $t('vendors.saving') : $t('common.save')}
            </button>
          </div>
        </form>
      </div>
    </div>
  </div>
{/if}
