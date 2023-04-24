import Nango from '@nangohq/frontend';

export function auth_provider(nangoHost, configKey, connectionId) {
  return new Nango({ host: nangoHost }).auth(configKey, connectionId);
}
