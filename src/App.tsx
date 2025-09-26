import { Layout } from './Layout';
import { ContextMenu } from './ContextMenu';
import { cx } from 'jinge';
import { IS_ANDROID } from './service/util';

function App() {
  return (
    <div className={cx('bg-background flex size-full overflow-hidden', IS_ANDROID && 'pt-8')}>
      <Layout />
      <ContextMenu />
    </div>
  );
}

export default App;
