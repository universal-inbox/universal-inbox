import Nango from '@nangohq/frontend';

export function auth_provider(nangoHost, publicKey, configKey, connectionId, oauthUserScopes) {
  return new Nango({ host: nangoHost, publicKey: publicKey, debug: true })
    .auth(configKey, connectionId, { user_scope: oauthUserScopes });
}

import Datepicker from 'flowbite-datepicker/Datepicker';

export { Datepicker };

export function init_headway() {
  if (typeof Headway === 'object') {
    Headway.init({
      selector: "#ui-changelog",
      account: "7Xr08y"
    });
  }
}

export function show_headway() {
  if (typeof Headway === 'object') {
    Headway.show();
  }
}
