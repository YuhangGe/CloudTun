import { createWriteStream } from 'node:fs';
import unzip, { type Entry, type ZipFile } from 'yauzl';
import path from 'node:path';
import { mkdir } from 'node:fs/promises';

/**
 * 跨平台的 unzip 方案。windows 系统下没有 unzip 命令。
 */
export function unzipFile(zipFilePath: string, targetDir: string) {
  async function handleEntry(entry: Entry, zipfile: ZipFile) {
    const fullPath = path.join(targetDir, entry.fileName);

    // 检查是否是目录
    if (entry.fileName.endsWith('/')) {
      await mkdir(fullPath, { recursive: true });
    } else {
      // 确保目录存在
      await mkdir(path.dirname(fullPath), { recursive: true });

      await new Promise<void>((resolve, reject) => {
        zipfile.openReadStream(entry, (err, readStream) => {
          if (err) return reject(err);

          readStream.on('error', reject);

          const writeStream = createWriteStream(fullPath);
          writeStream.on('error', reject);
          writeStream.on('close', () => resolve());

          readStream.pipe(writeStream);
        });
      });
    }
  }

  return new Promise((resolve, reject) => {
    unzip.open(zipFilePath, { lazyEntries: true }, (err, zipfile) => {
      if (err) return reject(err);

      zipfile.on('entry', (entry) => {
        handleEntry(entry, zipfile).then(
          () => {
            zipfile.readEntry(); // read next entry
          },
          (err) => {
            reject(err);
          },
        );
      });

      zipfile.on('error', reject);
      zipfile.on('end', resolve);
      zipfile.readEntry();
    });
  });
}
