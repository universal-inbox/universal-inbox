#!/usr/bin/env sh

set -euo pipefail

cd node_modules/@nangohq/frontend
npm install --dev
npm run build
cd -

# For some reason, `npx bun build:js` fails when run in place
rm -rf /tmp/flyonui
mv node_modules/flyonui /tmp/
cp scripts/flyonui-bun.lock /tmp/flyonui/bun.lock
cd /tmp/flyonui
npx bun install --dev
npx bun build:js
npx bun build:css
cd -
mv /tmp/flyonui node_modules/
