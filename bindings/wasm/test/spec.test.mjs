// Corre el subconjunto de `spec/cases/` que soporta el binding contra el `.wasm` (runner compartido).
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';
import { Geoda } from '../geoda.mjs';
import { registerSpecTests } from '../../spec-runner/run.mjs';

const geoda = await Geoda.load(fs.readFileSync(fileURLToPath(new URL('../geoda.wasm', import.meta.url))));
registerSpecTests(geoda, { minimum: 85 });
