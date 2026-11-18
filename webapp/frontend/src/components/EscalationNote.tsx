import type { AiEscalation } from "@/lib/types";
import { formatFullDateTime, formatMessageTime } from "@/lib/time";

// Categorías de la herramienta escalar_a_humano de ai-agents.
const CATEGORY_LABELS = new Map<string, string>([
  ["cancellation", "Cancelación"],
  ["shipping_issue", "Problema con el envío"],
  ["product_issue", "Problema con el producto"],
  ["product_question", "Pregunta sobre el producto"],
  ["opt_out", "No quiere más mensajes"],
  ["complaint", "Queja"],
  ["human_requested", "Pidió hablar con un asesor"],
  ["other", "Otro motivo"],
]);

/**
 * El agente de IA le pasó el chat a un asesor: categoría y motivo. Rojo
 * muy claro con texto rojo oscuro, para que resalte y se lea fácil.
 */
export default function EscalationNote({ escalation }: { escalation: AiEscalation }) {
  const category = CATEGORY_LABELS.get(escalation.category) ?? escalation.category;

  return (
    <div className="flex justify-center">
      <div className="max-w-[70%] rounded-lg border border-red-300 bg-red-50 px-3 py-2 text-red-950">
        <p className="text-[10px] font-semibold tracking-wider text-red-700 uppercase">Escalado a un asesor</p>
        <p className="mt-0.5 text-sm font-semibold wrap-break-word">{category}</p>
        <p className="mt-0.5 text-sm whitespace-pre-wrap wrap-break-word">{escalation.reason}</p>
        <div className="mt-1 flex items-center justify-end gap-1 text-[10px] text-red-800">
          {escalation.guide_number && (
            <span className="mr-auto pr-2 font-mono">Guía {escalation.guide_number}</span>
          )}
          <span title={formatFullDateTime(escalation.created_at)}>{formatMessageTime(escalation.created_at)}</span>
        </div>
      </div>
    </div>
  );
}
