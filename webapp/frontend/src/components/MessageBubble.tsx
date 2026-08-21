import type { Message } from "@/lib/types";
import { formatMessageTime } from "@/lib/time";

function StatusTicks({ status }: { status: Message["status"] }) {
  if (!status) return null;

  if (status === "sent") {
    return <span className="text-neutral-500">✓</span>;
  }
  if (status === "delivered") {
    return <span className="text-neutral-400">✓✓</span>;
  }
  return <span className="text-moon">✓✓</span>; // read
}

function MessageContent({ message }: { message: Message }) {
  if (message.media_type === "document") {
    return (
      <span className="italic">
        [Documento] {message.message ?? "Guía de envío"}
      </span>
    );
  }
  if (message.media_type === "audio") {
    return <span className="italic">[Audio]</span>;
  }
  if (message.media_type === "image") {
    return (
      <span className="italic">
        [Imagen] {message.message ?? ""}
      </span>
    );
  }
  return <span>{message.message}</span>;
}

export default function MessageBubble({ message }: { message: Message }) {
  const fromUser = message.from_user;

  return (
    <div className={`flex ${fromUser ? "justify-start" : "justify-end"}`}>
      <div
        className={`max-w-[70%] rounded-lg border px-3 py-2 ${
          fromUser
            ? "border-moon/20 bg-neutral-900 text-white"
            : "border-moon/40 bg-night text-white"
        }`}
      >
        <p className="text-sm wrap-break-word">
          <MessageContent message={message} />
        </p>
        <div className="mt-1 flex items-center justify-end gap-1 text-[10px] text-neutral-500">
          <span>{formatMessageTime(message.created_at)}</span>
          {!fromUser && <StatusTicks status={message.status} />}
        </div>
      </div>
    </div>
  );
}
