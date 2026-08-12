export type SpinnerSize = 'xs' | 'sm' | 'md' | 'lg';

export function Spinner({
  size = 'md',
  className = '',
}: {
  size?: SpinnerSize;
  className?: string;
}) {
  return <span className={`loading loading-spinner loading-${size} ${className}`} />;
}

export function PageLoader() {
  return (
    <div className="flex items-center justify-center py-16">
      <Spinner size="lg" className="text-primary" />
    </div>
  );
}
