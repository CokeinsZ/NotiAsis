import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "NotiAsis",
  description: "Notificaciones de guías de envío por WhatsApp",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="es">
      <body className="bg-black font-sans text-white antialiased">{children}</body>
    </html>
  );
}
