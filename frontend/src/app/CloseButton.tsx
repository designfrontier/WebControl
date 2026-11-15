import { useCloseModal } from "./modalHistory";

export const CloseButton = ({ children }: { children: string }) => {
  const close = useCloseModal();
  return (
    <button type="button" onClick={close}>
      {children}
    </button>
  );
};
