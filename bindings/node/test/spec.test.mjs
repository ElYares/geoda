// Corre el subconjunto de `spec/cases/` que soporta el binding (runner compartido con WASM).
import { createRequire } from 'node:module';
import { registerSpecTests } from '../../spec-runner/run.mjs';

const geoda = createRequire(import.meta.url)('..');
registerSpecTests(geoda, { minimum: 60 });
