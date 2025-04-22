import { Layout } from './Layout';
import { ContextMenu } from './ContextMenu';

function App() {
  return (
    <div className='bg-background flex size-full overflow-hidden'>
      <Layout />
      <ContextMenu />
    </div>
  );
}

export default App;
