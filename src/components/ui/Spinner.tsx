export type SpinnerSize = 'xs' | 'sm' | 'md' | 'lg';

// Never interpolate class names: Tailwind v4 scans source text, so a
// `loading-${size}` would never be generated.
const SIZE: Record<SpinnerSize, string> = {
  xs: 'loading-xs',
  sm: 'loading-sm',
  md: 'loading-md',
  lg: 'loading-lg',
};

export function Spinner({
  size = 'md',
  className = '',
}: {
  size?: SpinnerSize;
  className?: string;
}) {
  return <span className={`loading loading-spinner ${SIZE[size]} ${className}`} />;
}

export function PageLoader() {
  return (
    <div className="flex items-center justify-center py-16">
      <Spinner size="lg" className="text-primary" />
    </div>
  );
}
