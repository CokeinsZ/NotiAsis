from dataclasses import dataclass, field

import polars as pl

from app.core.interfaces import MediaUploader, MessageSender, NotificationBackend, SheetSource
from app.models.shipping import RecipientInfo
from app.services.notification_policy import NotificationStep, step_for_notification_count
from app.whatsapp.templates.base import TemplateMessage
from app.whatsapp.templates.guia import GuiaTemplate
from app.whatsapp.templates.mensaje_guia import MensajeGuiaTemplate
from app.whatsapp.templates.recordatorio import RecordatorioTemplate
from app.whatsapp.templates.recordatorio_final import RecordatorioFinalTemplate


@dataclass
class SheetNotificationReport:
    """Resumen de una corrida de notificación masiva desde el sheet."""

    total_pending: int = 0
    new_notifications: int = 0
    reminders: int = 0
    final_reminders: int = 0
    skipped: list[str] = field(default_factory=list)
    errors: list[str] = field(default_factory=list)


class SheetNotificationService:
    """Notificación masiva desde el Google Sheet de un business.

    Cruza la hoja `office` (todos los pedidos) con la hoja `delivered`
    (pedidos ya reclamados) por `numero_guia` y, por cada guía que aún
    no ha sido reclamada:

    - si la guía ya fue notificada antes (tabla guides) -> plantilla
      `recordatorio`;
    - si nunca fue notificada -> descarga el PDF de la columna
      "Guia pdf", lo sube a Meta y envía las plantillas `guia` +
      `mensaje_guia_es`, registrando la guía para no repetir.
    """

    def __init__(
        self,
        sheet_source: SheetSource,
        backend: NotificationBackend,
        message_sender: MessageSender,
        media_uploader: MediaUploader,
        debug_notification_number: str | None = None,
        notification_override_number: str | None = None,
    ) -> None:
        self._sheet_source = sheet_source
        self._backend = backend
        self._message_sender = message_sender
        self._media_uploader = media_uploader
        self._debug_notification_number = debug_notification_number
        self._notification_override_number = notification_override_number

    def notify_business_sheet(self, business_id: int) -> SheetNotificationReport:
        report = SheetNotificationReport()

        sheet = self._backend.get_business_sheet(business_id)
        if not sheet or not sheet.get("document_id"):
            report.errors.append(f"Business {business_id} has no sheet config")
            return report
        if not sheet.get("office_id") or not sheet.get("delivered_id"):
            report.errors.append(f"Business {business_id} sheet config is incomplete")
            return report

        document_id = sheet["document_id"]
        office = self._sheet_source.fetch_sheet(document_id, sheet["office_id"])
        delivered = self._sheet_source.fetch_sheet(document_id, sheet["delivered_id"])

        pending = self._pending_guides(office, delivered)
        report.total_pending = pending.height
        print(f"Sheet {document_id}: {pending.height} pendientes por reclamar")

        for row in pending.iter_rows(named=True):
            self._process_row(business_id, row, report)

        print(
            f"Sheet notification done for business {business_id}: "
            f"{report.new_notifications} nuevas, {report.reminders} recordatorios, "
            f"{report.final_reminders} recordatorios finales, "
            f"{len(report.skipped)} omitidas, {len(report.errors)} errores"
        )
        return report

    @staticmethod
    def _pending_guides(office: pl.DataFrame, delivered: pl.DataFrame) -> pl.DataFrame:
        """Guías que están en office pero NO en delivered (anti-join).

        Las filas sin numero_guia se excluyen del cruce (los vacíos
        harían match entre sí).
        """
        office_valid = office.filter(pl.col("numero_guia").is_not_null() & (pl.col("numero_guia") != ""))
        delivered_valid = delivered.filter(pl.col("numero_guia").is_not_null() & (pl.col("numero_guia") != ""))
        return office_valid.join(delivered_valid, on="numero_guia", how="anti")

    def _process_row(self, business_id: int, row: dict, report: SheetNotificationReport) -> None:
        recipient = self._build_recipient(row)
        if recipient is None or not recipient.is_complete:
            report.skipped.append(str(row.get("numero_guia") or row.get("NOMBRE") or "?"))
            return

        tracking = recipient.tracking_number
        try:
            guide = self._backend.get_guide(tracking)
            count = (guide.get("notification_count") or 0) if guide else 0
            step = step_for_notification_count(count)

            if step is NotificationStep.STOP:
                report.skipped.append(tracking)
            elif step is NotificationStep.INITIAL:
                self._send_new_notification(business_id, recipient, row, report)
            else:
                self._send_reminder(business_id, recipient, step, report)
        except Exception as e:
            print(f"Error processing guide {tracking}: {e}")
            report.errors.append(tracking)

    def _deliver(
        self,
        recipient: RecipientInfo,
        templates: list[TemplateMessage],
    ) -> list[tuple[TemplateMessage, str]]:
        """Envía las plantillas en orden y retorna las enviadas con sus meta_ids.

        Con NOTIFICATION_OVERRIDE_NUMBER configurado, el envío físico se
        desvía a ese número (pruebas); el historial siempre se registra
        con el teléfono real del destinatario. La copia de depuración
        (DEBUG_NOTIFICATION_NUMBER) se envía además si está configurada.
        """
        target_number = self._notification_override_number or recipient.phone
        if target_number != recipient.phone:
            print(f"[OVERRIDE] {recipient.tracking_number}: envío desviado a {target_number}")

        sent: list[tuple[TemplateMessage, str]] = []
        for template in templates:
            meta_message_id = self._message_sender.send_template(target_number, template)
            if meta_message_id:
                sent.append((template, meta_message_id))

        debug_number = self._debug_notification_number
        if debug_number and debug_number != target_number:
            for template in templates:
                self._message_sender.send_template(debug_number, template)

        return sent

    def _register_outgoing(
        self,
        business_id: int,
        recipient: RecipientInfo,
        sent: list[tuple[TemplateMessage, str]],
    ) -> None:
        for template, meta_message_id in sent:
            self._backend.register_outgoing_message(
                business_id=business_id,
                user_phone=recipient.phone,
                user_name=recipient.name,
                meta_message_id=meta_message_id,
                media_type=template.media_type(),
                message=template.log_message(),
                media_id=template.log_media_id(),
            )

    def _send_reminder(
        self,
        business_id: int,
        recipient: RecipientInfo,
        step: NotificationStep,
        report: SheetNotificationReport,
    ) -> None:
        factory = (
            RecordatorioTemplate
            if step is NotificationStep.REMINDER
            else RecordatorioFinalTemplate
        )
        sent = self._deliver(recipient, [factory(recipient)])
        if not sent:
            report.errors.append(recipient.tracking_number)
            return

        self._register_outgoing(business_id, recipient, sent)
        self._backend.mark_guide_notified(recipient.tracking_number)
        if step is NotificationStep.REMINDER:
            report.reminders += 1
        else:
            report.final_reminders += 1

    def _send_new_notification(
        self,
        business_id: int,
        recipient: RecipientInfo,
        row: dict,
        report: SheetNotificationReport,
    ) -> None:
        tracking = recipient.tracking_number

        pdf_url = str(row.get("Guia pdf") or "").strip()
        pdf_bytes = self._sheet_source.download_file(pdf_url) if pdf_url else None
        if not pdf_bytes:
            print(f"Guide {tracking}: could not download PDF from {pdf_url!r}")
            report.errors.append(tracking)
            return

        media_id = self._media_uploader.upload_media(pdf_bytes, f"Guia_{tracking}.pdf")
        if not media_id:
            report.errors.append(tracking)
            return

        if not self._backend.register_guide(tracking, recipient.phone, recipient.name):
            # Carrera rara: otro proceso la registró mientras tanto.
            report.skipped.append(tracking)
            return

        templates = [GuiaTemplate(recipient, media_id), MensajeGuiaTemplate(recipient, media_id)]
        sent = self._deliver(recipient, templates)

        self._register_outgoing(business_id, recipient, sent)
        if len(sent) < len(templates):
            report.errors.append(tracking)
            return

        self._backend.mark_guide_notified(tracking)
        report.new_notifications += 1

    @staticmethod
    def _build_recipient(row: dict) -> RecipientInfo | None:
        """Mapea una fila de la hoja office a RecipientInfo.

        Columnas usadas: NOMBRE, TELEFONO, DIRECCION, CIUDAD,
        DEPARTAMENTO, PRODUCTO, numero_guia.
        """
        tracking = str(row.get("numero_guia") or "").strip()
        if not tracking:
            return None

        address = str(row.get("DIRECCION") or "").strip()
        city = str(row.get("CIUDAD") or "").strip()
        department = str(row.get("DEPARTAMENTO") or "").strip()
        if city:
            address = f"{address}, {city}"
            if department:
                address = f"{address}, {department}"

        return RecipientInfo(
            name=str(row.get("NOMBRE") or "").strip(),
            phone=str(row.get("TELEFONO") or "").strip(),
            tracking_number=tracking,
            delivery_address=address,
            product=str(row.get("PRODUCTO") or "").strip(),
        )
