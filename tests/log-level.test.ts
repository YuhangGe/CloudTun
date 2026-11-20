import { expect, test } from 'bun:test';

import { logTone } from '../src/service/log-level';

test.each([
  [
    '[proxy::warn] CONNECT tunnel closed: Custom { kind: UnexpectedEof, error: "early eof" }',
    'warning',
  ],
  ['[proxy::warn] HTTP connection closed: hyper::Error(IncompleteMessage)', 'warning'],
  ['[proxy::error] stage=outbound.connect timeout elapsed_ms=15002', 'error'],
  ['[proxy::error] VMess authentication failed', 'error'],
  ['[proxy::info] Proxy ==> error.example.com:443', 'normal'],
  ['[proxy::debug] stage=connect.cancel reason=proxy-stopped', 'normal'],
  ['[log::geo] GEO 下载超时', 'error'],
  ['已加载本地设置', 'normal'],
])('log severity follows its explicit level: %s', (text, expected) => {
  expect(logTone(text)).toBe(expected);
});
