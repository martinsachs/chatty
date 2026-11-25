# Änderungen

## 0.2.5 – 2026-11-25

**Neu** (beide Varianten)
- Text kopieren: im Verlauf mit der Maus markieren und Strg+C, ⎘ an jeder Nachricht kopiert sie ganz.
  Native Variante markiert je Absatz; „Als Text“ im Kanalkopf (Strg+Umschalt+A) zeigt den Verlauf als einen
  markierbaren Text.
- Bilder aus der Zwischenablage mit Strg+V als Anhang einfügen, Vorschaubild über dem Eingabefeld (auch für
  per 📎 gewählte Bilder).
- 👁 zeigt den getippten Text formatiert vor dem Senden.

**Neu** (Tauri-Variante; native nur Farben der Optik und Link auf die Google-Anleitung)
- Optik „seequ“ im Stil eines Messengers von 2001.
- Online-Status (Mattermost): Punkt an jeder DM, Statusmenü im Kontokopf mit sieben Stufen.
- Ton bei Direktnachrichten und Erwähnungen, abschaltbar (Vorgabe: nur in seequ an).
- Google-Anmeldung: ⓘ mit Kurzanleitung und Direktlinks in die Cloud Console; Passwort und Client-Secret
  mit Augen-Knopf anzeigbar; Zahnrad unten in der Kontoleiste öffnet die Einstellungen.

## 0.2.4 – 2026-11-24

**Neu** (beide Varianten)
- Schriftgröße einstellbar (80–200 %, Darstellung oder Strg + Plus/Minus/0).
- Angehängte Bilder als Vorschau im Verlauf; Klick speichert das Original.
- Emojis größer: im Text 1,3-fach, Nachrichten nur aus Emojis groß. Native Variante zeigt Emojis im Text
  jetzt farbig.

## 0.2.3 – 2026-11-23

**Sicherheit** (Details: `konzept/sicherheit.md`)
- Behoben: HTML-Injection in Nachrichten und Kanal-Header der Tauri-Variante (seit 0.2.2). Eine präparierte
  Nachricht konnte einen unsichtbaren Link über das ganze Fenster legen. Danke an die meldende Person für
  Bericht und Patch.
- Mattermost nur noch über `https://` (`http://` nur für localhost).
- Selbst-Update der nativen Variante: Schutz gegen Downgrade auf ältere signierte Versionen, nur https,
  Größenlimit.
- Release-Pipeline: Actions auf Commit gepinnt, Signaturschlüssel nur noch in zwei abgeschotteten Jobs.
- Härtungen: Obergrenzen und Timeouts für Downloads und WebSocket, Schutz vor Bild-Bomben (native), kein
  Markup in Systembenachrichtigungen, Dateinamen ohne Richtungs-Steuerzeichen und Pfade, Konto entfernen
  meldet die Sitzung beim Server ab, engere Fensterrechte und CSP, Google-Anmeldung robuster gegen lokale
  Störungen.
- Abhängigkeiten aktualisiert (`url`, `cookie`), `cargo audit` und `npm audit` in CI.
- Lücken privat melden: siehe `SECURITY.md`.

## 0.2.2 – 2026-11-23

**Neu**
- Mattermost: Der Kanal-Header steht unter dem Kanalnamen, Links lassen sich anklicken. Ändert jemand
  den Header, aktualisiert er sich sofort. In mIRC- und DOS-Optik steht er einzeilig wie ein IRC-Topic
  („Kanal: Thema“). Gilt für beide Varianten (Tauri und native).
- Links in der Form `[Text](https://…)` werden in Nachrichten klickbar.
- Neues App-Icon (Phosphor „chats-circle“, MIT) im ehex-Verlauf Blau → Grün.

**Doku**
- README mit Bildern aller fünf Optiken, Hinweis auf die Entwicklung mit KI (Claude Code),
  rechtliche Hinweise.

## 0.2.1 – 2026-11-21

- Mattermost: Thread-Antworten stehen auch im Hauptverlauf, links mit Balken markiert.
- mIRC- und DOS-Optik: ungelesene Kanäle mit „*“ und Farbe markiert.

## 0.2.0 – 2026-11-21

- Native Variante `chatty-native` (iced) für Linux, Windows und macOS, mit Installern und Selbst-Update.
- Optiken mIRC und DOS, ehex-Farben aus dem Dark Mode von ehex.de.
- Nachrichtenfenster: ältere Nachrichten erst beim Hochscrollen nachladen.
- Seitenleiste in der Breite verstellbar, Sortierung, alte DMs ausgeblendet.
- Hauptfenster startet mit 2/3 der Bildschirmgröße.
