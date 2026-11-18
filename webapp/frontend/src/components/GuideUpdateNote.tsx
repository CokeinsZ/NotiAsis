import type { GuideCustomerUpdate } from "@/lib/types";
import { formatCalendarDay, formatFullDateTime, formatMessageTime } from "@/lib/time";

function summary(update: GuideCustomerUpdate): string {
  switch (update.kind) {
    case "will_pickup":
      return update.promised_date
        ? `Recoge el pedido el ${formatCalendarDay(update.promised_date)}`
        : "Va a recoger el pedido";
    case "picked_up":
      return "Ya recogió el pedido";
    case "declined":
      return "No quiere el pedido";
    default:
      return update.kind;
  }
}

/**
 * Novedad de una guía que el agente de IA registró con lo que le dijo el
 * cliente. Es interna (el cliente no la ve): va centrada y en gris para
 * no confundirla con un mensaje de WhatsApp.
 */
export default function GuideUpdateNote({ update }: { update: GuideCustomerUpdate }) {
  return (
    <div className="flex justify-center">
      <div className="max-w-[70%] rounded-lg border border-neutral-600 bg-neutral-800 px-3 py-2 text-neutral-100">
        <p className="text-[10px] font-semibold tracking-wider text-neutral-400 uppercase">Nota de la IA</p>
        <p className="mt-0.5 text-sm font-medium wrap-break-word">{summary(update)}</p>
        {update.note && (
          <p className="mt-0.5 text-sm whitespace-pre-wrap text-neutral-300 wrap-break-word">{update.note}</p>
        )}
        <div className="mt-1 flex items-center justify-end gap-1 text-[10px] text-neutral-400">
          <span className="mr-auto pr-2 font-mono">Guía {update.guide_number}</span>
          <span title={formatFullDateTime(update.created_at)}>{formatMessageTime(update.created_at)}</span>
        </div>
      </div>
    </div>
  );
}
