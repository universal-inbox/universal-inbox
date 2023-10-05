import Nango from '@nangohq/frontend';

export function auth_provider(nangoHost, publicKey, configKey, connectionId) {
  return new Nango({ host: nangoHost, publicKey: publicKey }).auth(configKey, connectionId);
}
