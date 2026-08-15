import { useEffect, useRef, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { X } from 'lucide-react';
import { Button } from './Button';

const MODAL_SIZE = { sm: 'max-w-sm', md: '', lg: 'max-w-2xl', xl: 'max-w-4xl' } as const;

export function Modal({
  open,
  onClose,
  title,
  size = 'md',
  dismissable = true,
  actions,
  className = '',
  children,
}: {
  open: boolean;
  onClose: () => void;
  title?: ReactNode;
  size?: keyof typeof MODAL_SIZE;
  dismissable?: boolean;
  actions?: ReactNode;
  className?: string;
  children: ReactNode;
}) {
  const { t } = useTranslation('common');
  const ref = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  }, [open]);

  useEffect(() => {
    const dialog = ref.current;
    if (!dialog) return;
    const handleCancel = (event: Event) => {
      event.preventDefault();
      onClose();
    };
    dialog.addEventListener('cancel', handleCancel);
    return () => dialog.removeEventListener('cancel', handleCancel);
  }, [onClose]);

  return (
    <dialog ref={ref} className="modal modal-bottom sm:modal-middle">
      <div className={`modal-box ${MODAL_SIZE[size]} ${className}`}>
        {(title || dismissable) && (
          <div className="flex items-start justify-between gap-4 mb-4">
            {title ? <h3 className="text-lg font-bold">{title}</h3> : <span />}
            {dismissable && (
              <Button
                appearance="ghost"
                size="sm"
                shape="circle"
                aria-label={t('close')}
                onClick={onClose}
              >
                <X className="w-4 h-4" />
              </Button>
            )}
          </div>
        )}
        {children}
        {actions && <div className="modal-action">{actions}</div>}
      </div>
      {dismissable && (
        <form method="dialog" className="modal-backdrop">
          <button type="button" onClick={onClose}>
            close
          </button>
        </form>
      )}
    </dialog>
  );
}
