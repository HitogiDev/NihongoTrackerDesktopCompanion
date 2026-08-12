import type { ButtonHTMLAttributes, ReactNode } from 'react';
import { Spinner, type SpinnerSize } from './Spinner';

// Class order is always: btn -> colour -> style -> size -> shape.
// Never interpolate class names: Tailwind v4 scans source text.
type Variant = 'primary' | 'secondary' | 'accent' | 'error' | 'success' | 'warning' | 'neutral';
type Appearance = 'solid' | 'outline' | 'ghost' | 'soft' | 'link';
type Size = 'xs' | 'sm' | 'md' | 'lg';
type Shape = 'default' | 'square' | 'circle';

const VARIANT: Record<Variant, string> = {
  primary: 'btn-primary',
  secondary: 'btn-secondary',
  accent: 'btn-accent',
  error: 'btn-error',
  success: 'btn-success',
  warning: 'btn-warning',
  neutral: 'btn-neutral',
};

const APPEARANCE: Record<Appearance, string> = {
  solid: '',
  outline: 'btn-outline',
  ghost: 'btn-ghost',
  soft: 'btn-soft',
  link: 'btn-link',
};

const SIZE: Record<Size, string> = {
  xs: 'btn-xs',
  sm: 'btn-sm',
  md: '',
  lg: 'btn-lg',
};

const SHAPE: Record<Shape, string> = {
  default: '',
  square: 'btn-square',
  circle: 'btn-circle',
};

const BTN_SPINNER: Record<Size, SpinnerSize> = { xs: 'xs', sm: 'xs', md: 'sm', lg: 'md' };

export interface ButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'children'> {
  variant?: Variant;
  appearance?: Appearance;
  size?: Size;
  shape?: Shape;
  block?: boolean;
  loading?: boolean;
  children?: ReactNode;
}

export function Button({
  variant,
  appearance = 'solid',
  size = 'md',
  shape = 'default',
  block = false,
  loading = false,
  className = '',
  disabled,
  children,
  ...rest
}: ButtonProps) {
  const classes = [
    'btn',
    variant ? VARIANT[variant] : '',
    APPEARANCE[appearance],
    SIZE[size],
    SHAPE[shape],
    block ? 'btn-block' : '',
    className,
  ]
    .filter(Boolean)
    .join(' ');

  return (
    // daisyUI v5 dropped `loading` as a btn modifier — render a spinner instead.
    <button className={classes} disabled={disabled || loading} {...rest}>
      {loading ? <Spinner size={BTN_SPINNER[size]} /> : null}
      {children}
    </button>
  );
}
