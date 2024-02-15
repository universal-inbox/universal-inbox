import Nango from '@nangohq/frontend';

export function auth_provider(nangoHost, publicKey, configKey, connectionId, oauthUserScopes) {
  return new Nango({ host: nangoHost, publicKey: publicKey, debug: true })
    .auth(configKey, connectionId, { user_scope: oauthUserScopes });
}

import Datepicker from 'flowbite-datepicker/Datepicker';

export { Datepicker };
