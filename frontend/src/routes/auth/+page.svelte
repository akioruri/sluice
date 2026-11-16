<script lang="ts">
  import { setSession } from '$lib/stores';
  import { LOCALES, LOCALE_LABELS, locale, t } from '$lib/i18n';
  import { icons } from '$lib/icons';

  let username = $state('root');
  let password = $state('');
  let error = $state('');
  let submitting = $state(false);

  async function login(e: SubmitEvent) {
    e.preventDefault();
    error = '';
    if (!username.trim() || !password) {
      error = $t('login.required');
      return;
    }

    submitting = true;
    try {
      const { api } = await import('$lib/stores');
      const token = await api.login(username.trim(), password);
      setSession(token);
      window.location.href = '/dashboard';
    } catch (err) {
      error = err instanceof Error ? err.message : $t('login.failed');
    } finally {
      submitting = false;
    }
  }
</script>

<div class="min-h-screen flex items-center justify-center bg-canvas px-4">
  <div class="w-full max-w-sm">
    <div class="flex justify-end mb-6">
      <select
        bind:value={$locale}
        aria-label={$t('nav.language')}
        class="bg-surface border border-line rounded-lg px-2 py-1.5 text-xs text-muted focus:outline-none focus:ring-1 focus:ring-accent"
      >
        {#each LOCALES as code}
          <option value={code}>{LOCALE_LABELS[code]}</option>
        {/each}
      </select>
    </div>

    <div class="bg-surface border border-line rounded-xl px-7 py-8">
      <div class="flex items-center gap-2.5 mb-7">
        <span class="w-8 h-8 rounded-lg bg-accent flex items-center justify-center text-accent-ink">
          <span class="w-4.5 h-4.5 [&>svg]:w-full [&>svg]:h-full">{@html icons.plug}</span>
        </span>
        <span class="font-semibold text-ink">{$t('app.name')}</span>
      </div>

      <h2 class="text-lg font-semibold text-ink">{$t('login.subtitle')}</h2>

      <form onsubmit={login} class="mt-6 space-y-4">
        {#if error}
          <div class="flex items-start gap-2 bg-danger/10 border border-danger/30 text-danger px-3 py-2.5 rounded-lg text-sm">
            <span class="w-4 h-4 mt-0.5 shrink-0 [&>svg]:w-full [&>svg]:h-full">{@html icons.alert}</span>
            {error}
          </div>
        {/if}
        <div>
          <label for="username" class="block text-sm font-medium text-ink mb-1.5">{$t('login.username')}</label>
          <input
            id="username"
            type="text"
            bind:value={username}
            autocomplete="username"
            class="block w-full px-3 py-2 bg-canvas border border-line rounded-lg text-sm text-ink placeholder:text-faint focus:outline-none focus:ring-1 focus:ring-accent"
            placeholder="root"
          />
        </div>
        <div>
          <label for="password" class="block text-sm font-medium text-ink mb-1.5">{$t('login.password')}</label>
          <input
            id="password"
            type="password"
            bind:value={password}
            autocomplete="current-password"
            class="block w-full px-3 py-2 bg-canvas border border-line rounded-lg text-sm text-ink placeholder:text-faint focus:outline-none focus:ring-1 focus:ring-accent"
            placeholder={$t('login.password')}
          />
        </div>
        <button
          type="submit"
          disabled={submitting}
          class="w-full py-2 rounded-lg text-sm font-medium bg-ink text-canvas hover:opacity-90 disabled:opacity-50 transition-opacity"
        >
          {submitting ? $t('login.submitting') : $t('login.submit')}
        </button>
      </form>
    </div>
  </div>
</div>
