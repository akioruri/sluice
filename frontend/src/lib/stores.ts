import { AdminApi } from './admin-api';

const SESSION_KEY = 'sluice_session';

export let sessionToken = localStorage.getItem(SESSION_KEY) || '';

export let api = new AdminApi(sessionToken);

export function setSession(token: string) {
  sessionToken = token;
  api = new AdminApi(token);
  localStorage.setItem(SESSION_KEY, token);
}

export function clearSession() {
  sessionToken = '';
  api = new AdminApi('');
  localStorage.removeItem(SESSION_KEY);
}

export function isLoggedIn(): boolean {
  return sessionToken.length > 0;
}
