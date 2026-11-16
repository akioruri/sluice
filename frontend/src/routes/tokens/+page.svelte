<script lang="ts">
  import { api } from '$lib/stores';
  import { t } from '$lib/i18n';
  import { icons } from '$lib/icons';
  import type { TokenInfo, CreatedToken } from '$lib/admin-api';

  let tokens = $state<TokenInfo[]>([]);
  let loading = $state(true);
  let error = $state('');
  let creating = $state(false);
  let created = $state<CreatedToken | null>(null);
  let copied = $state(false);
  let newName = $state('');

  // Vendor used in the curl example; kept in sync with the vendors page.
  let exampleVendor = $state('vendor');
  let exampleModel = $state('gpt-4o-mini');

  $effect(() => {
    loadTokens();
    loadExampleVendor();
  });

  async function loadTokens() {
    try {
      tokens = await api.getTokens();
    } catch (e) {
      error = e instanceof Error ? e.message : $t('tokens.loadFailed');
    } finally {
      loading = false;
    }
  }

  async function loadExampleVendor() {
    try {
      const vendors = await api.getProviders();
      if (vendors.length > 0) {
        exampleVendor = vendors[0].name;
        const first = vendors[0].models.split(',').map((m) => m.trim()).filter(Boolean);
        if (first.length > 0) {
          exampleModel = first[0];
        }
      }
    } catch {
      // Non-fatal: the curl example just keeps its placeholder values.
    }
  }

  /**
   * Creates a key. The name must be unique among the keys still in use.
   */
  async function createToken() {
    const name = newName.trim();
    if (!name) return;
    if (tokens.some((tk) => tk.name === name)) {
      error = $t('tokens.nameDuplicate');
      return;
    }

    creating = true;
    error = '';
    try {
      created = await api.createToken(name);
      newName = '';
      copied = false;
      await loadTokens();
    } catch (e) {
      error = e instanceof Error ? e.message : $t('tokens.createFailed');
    } finally {
      creating = false;
    }
  }

  async function copyToken() {
    if (!created) return;
    await navigator.clipboard.writeText(created.token);
    copied = true;
  }

  async function revoke(prefix: string) {
    error = '';
    try {
      await api.revokeToken(prefix);
      await loadTokens();
    } catch (e) {
      error = e instanceof Error ? e.message : $t('tokens.revokeFailed');
    }
  }

  function curlExample(): string {
    const key = created ? created.token : 'sk-YOUR_API_KEY';
    return `curl ${window.location.origin}/${exampleVendor}/v1/chat/completions \\
  -H "Authorization: Bearer ${key}" \\
  -H "Content-Type: application/json" \\
  -d '{
    "model": "${exampleModel}",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'`;
  }

  /** Shows only the head and tail of a key, like the reference console. */
  function maskKey(prefix: string): string {
    if (prefix.length <= 10) return prefix;
    return `${prefix.slice(0, 7)}${'*'.repeat(4)}${prefix.slice(-4)}`;
  }
</script>

<div class="max-w-5xl">
  <div class="mb-8">
    <h1 class="text-2xl font-semibold text-ink">{$t('tokens.title')}</h1>
    <p class="mt-2 text-sm text-muted max-w-2xl leading-relaxed">
      {$t('tokens.generateHint')}
    </p>
  </div>

  {#if error}
    <div class="flex items-start gap-2 bg-danger/10 border border-danger/30 text-danger px-4 py-3 rounded-lg mb-6 text-sm">
      <span class="w-4 h-4 mt-0.5 shrink-0 [&>svg]:w-full [&>svg]:h-full">{@html icons.alert}</span>
      {error}
    </div>
  {/if}

  <div class="bg-surface border border-line rounded-xl p-5 mb-6">
    <label for="newName" class="block text-sm font-medium text-ink mb-2">{$t('tokens.name')}</label>
    <div class="flex gap-2">
      <input
        id="newName"
        type="text"
        bind:value={newName}
        placeholder={$t('tokens.name')}
        onkeydown={(e) => e.key === 'Enter' && (e.preventDefault(), createToken())}
        class="flex-1 px-3 py-2 bg-canvas border border-line rounded-lg text-sm text-ink placeholder:text-faint focus:outline-none focus:ring-1 focus:ring-accent"
      />
      <button
        onclick={createToken}
        disabled={creating || !newName.trim()}
        class="shrink-0 px-4 py-2 rounded-lg text-sm font-medium bg-ink text-canvas hover:opacity-90 disabled:opacity-40 disabled:cursor-not-allowed transition-opacity"
      >
        {creating ? $t('tokens.generating') : $t('tokens.generateBtn')}
      </button>
    </div>
    {#if !newName.trim()}
      <p class="mt-2 text-xs text-faint">{$t('tokens.nameRequired')}</p>
    {/if}
  </div>

  {#if created}
    <div class="bg-success/10 border border-success/30 rounded-xl p-5 mb-6">
      <h3 class="text-sm font-medium text-ink mb-3">{$t('tokens.newKey')}</h3>
      <div class="flex items-center gap-2">
        <code class="flex-1 bg-canvas border border-line rounded-lg px-3 py-2 font-mono text-sm text-ink break-all">{created.token}</code>
        <button
          onclick={copyToken}
          class="inline-flex items-center gap-1.5 px-3 py-2 border border-line rounded-lg text-sm font-medium text-ink hover:bg-hover transition-colors"
        >
          <span class="w-4 h-4 [&>svg]:w-full [&>svg]:h-full">{@html copied ? icons.check : icons.copy}</span>
          {copied ? $t('common.copied') : $t('common.copy')}
        </button>
      </div>
    </div>

    <div class="bg-surface border border-line rounded-xl p-5 mb-6">
      <h3 class="text-sm font-medium text-ink mb-2">{$t('tokens.howTo')}</h3>
      <p class="text-sm text-muted mb-3">
        {$t('tokens.howToHint', { vendor: exampleVendor })}
      </p>
      <pre class="bg-canvas border border-line rounded-lg p-4 overflow-x-auto text-xs font-mono text-ink leading-relaxed">{curlExample()}</pre>
    </div>
  {/if}

  <div class="bg-surface border border-line rounded-xl overflow-hidden">
    <table class="w-full table-fixed">
      <thead>
        <tr class="border-b border-line">
          <th class="px-5 py-3 text-left text-xs font-medium text-muted truncate">{$t('tokens.name')}</th>
          <th class="px-5 py-3 text-left text-xs font-medium text-muted truncate">{$t('tokens.key')}</th>
          <th class="px-5 py-3 text-left text-xs font-medium text-muted truncate">{$t('tokens.createdAt')}</th>
          <th class="px-5 py-3 text-right text-xs font-medium text-muted w-28">{$t('tokens.status')}</th>
        </tr>
      </thead>
      <tbody>
        {#each tokens as token}
          <tr class="border-b border-subtle last:border-0 hover:bg-hover transition-colors">
            <td class="px-5 py-3.5 text-sm text-ink truncate">{token.name || '—'}</td>
            <td class="px-5 py-3.5 text-sm text-ink font-mono truncate">{maskKey(token.prefix)}</td>
            <td class="px-5 py-3.5 text-sm text-muted truncate">{new Date(token.created_at).toLocaleString()}</td>
            <td class="px-5 py-3.5 text-right">
              <button
                onclick={() => revoke(token.prefix)}
                class="inline-flex items-center gap-1.5 text-sm text-danger hover:opacity-80 transition-opacity"
              >
                <span class="w-4 h-4 [&>svg]:w-full [&>svg]:h-full">{@html icons.trash}</span>
                {$t('tokens.revoke')}
              </button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#if loading}
      <div class="px-5 py-8 text-center text-sm text-muted">{$t('common.loading')}</div>
    {:else if tokens.length === 0}
      <div class="px-5 py-8 text-center text-sm text-muted">{$t('tokens.none')}</div>
    {/if}
  </div>
</div>
