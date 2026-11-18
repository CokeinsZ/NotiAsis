/**
 * Día de los mensajes que siguen ("Dom 15 Nov"). Se queda pegado arriba
 * mientras se recorren los mensajes de ese día, como en WhatsApp.
 */
export default function DaySeparator({ label }: { label: string }) {
  return (
    <div className="sticky top-0 z-10 flex justify-center py-1">
      <span className="rounded-full border border-moon/25 bg-neutral-900 px-3 py-0.5 text-xs font-medium text-neutral-300 shadow-md shadow-black">
        {label}
      </span>
    </div>
  );
}
