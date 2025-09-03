#!/usr/bin/env sh

set -euo pipefail

cd node_modules/@nangohq/frontend
npm install --dev
npm run build
cd -
