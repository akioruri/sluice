<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { api, clearSession, isLoggedIn } from '$lib/stores';
  import { LOCALES, LOCALE_LABELS, locale, t } from '$lib/i18n';
  import { icons } from '$lib/icons';
  import type { IconName } from '$lib/icons';
  import '../app.css';
  import type { Snippet } from 'svelte';

  let { children }: { children: Snippet } = $props();

  // `page` from $app/state is a reactive object, not a store — read it directly.
  let pathname = $derived(page.url.pathname);

  const isLoginPage = $derived(pathname === '/' || pathname === '/auth');

  // Send unauthenticated visitors to the login page.
  $effect(() => {
    if (!isLoginPage && !isLoggedIn()) {
      goto('/');
    }
  });

  async function logout() {
    try {
      await api.logout();
    } finally {
      clearSession();
      goto('/');
    }
  }

  const navItems: { href: string; label: string; icon: IconName }[] = $derived([
    { href: '/dashboard', label: $t('nav.dashboard'), icon: 'chart' },
    { href: '/vendors', label: $t('nav.vendors'), icon: 'vendor' },
    { href: '/tokens', label: $t('nav.tokens'), icon: 'key' },
  ]);

  function navClass(target: string): string {
    const base = 'flex items-center gap-3 px-3 py-2 rounded-lg text-sm font-medium transition-colors';
    return pathname === target
      ? `${base} bg-active text-ink`
      : `${base} text-muted hover:bg-hover hover:text-ink`;
  }
</script>

{#if isLoginPage}
  {@render children()}
{:else}
  <div class="flex bg-canvas">
    <!-- Sidebar: pinned to the viewport so its position never depends on how
         tall the main content is. -->
    <aside class="w-60 shrink-0 sticky top-0 h-screen flex flex-col bg-sidebar border-r border-line">
      <div class="h-16 flex items-center gap-2 px-5 border-b border-line">
        <span class="w-7 h-7 rounded-lg bg-accent flex items-center justify-center text-accent-ink">
          <span class="w-4 h-4 [&>svg]:w-full [&>svg]:h-full">{@html icons.plug}</span>
        </span>
        <span class="font-semibold text-ink">{$t('app.name')}</span>
      </div>

      <nav class="flex-1 px-3 py-4 space-y-1 overflow-y-auto">
        {#each navItems as item}
          <a href={item.href} class={navClass(item.href)}>
            <span class="w-4.5 h-4.5 shrink-0 [&>svg]:w-full [&>svg]:h-full">{@html icons[item.icon]}</span>
            {item.label}
          </a>
        {/each}
      </nav>

      <!-- Utility + account -->
      <div class="px-3 py-4 border-t border-line space-y-1">
        <div class="flex items-center justify-between px-3 py-1">
          <span class="flex items-center gap-2 text-xs text-faint">
            <span class="w-3.5 h-3.5 [&>svg]:w-full [&>svg]:h-full">{@html icons.globe}</span>
            {$t('nav.language')}
          </span>
          <select
            bind:value={$locale}
            aria-label={$t('nav.language')}
            class="bg-transparent text-xs text-muted border border-line rounded-md px-1.5 py-1 focus:outline-none focus:ring-1 focus:ring-accent"
          >
            {#each LOCALES as code}
              <option value={code}>{LOCALE_LABELS[code]}</option>
            {/each}
          </select>
        </div>

        <div class="flex items-center justify-between px-3 py-2">
          <span class="flex items-center gap-2 text-sm text-muted">
            <span class="w-4.5 h-4.5 shrink-0 [&>svg]:w-full [&>svg]:h-full">{@html icons.user}</span>
            {$t('nav.adminUser')}
          </span>
          <button
            onclick={logout}
            title={$t('nav.signOut')}
            aria-label={$t('nav.signOut')}
            class="p-1.5 rounded-md text-muted hover:bg-hover hover:text-ink transition-colors"
          >
            <span class="w-4 h-4 block [&>svg]:w-full [&>svg]:h-full">{@html icons.logout}</span>
          </button>
        </div>
      </div>
    </aside>

    <!-- Main -->
    <main class="flex-1 min-w-0">
      <div class="max-w-6xl mx-auto px-8 py-10">
        {@render children()}
      </div>
    </main>
  </div>
{/if}
