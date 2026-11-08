# تشغيل BODKA Beta 02 على ويندوز

الإصدار `v0.2.0-beta.2` مخصّص لويندوز 10 و11 على أجهزة Intel وAMD بمعمارية x64. [تحميل الحزمة الكاملة (50.5 MiB)](https://github.com/aboodksa4011/Horizon-Bodka-Ultra-Beta/releases/download/v0.2.0-beta.2/Horizon-Bodka-Ultra-Beta-Frosted-Windows-x64.zip) · [صفحة الإصدار](https://github.com/aboodksa4011/Horizon-Bodka-Ultra-Beta/releases/tag/v0.2.0-beta.2)

## التثبيت أو تحديث Beta 01

1. نزّل حزمة ZIP. من مستكشف الملفات، اضغط بزر الفأرة الأيمن واختر **استخراج الكل** إلى مجلد تختاره.
2. إذا كنت تستخدم نسخة سابقة، أغلق نافذتها ثم افتح `HorizonBodkaUltra.exe` من المجلد الجديد. يخزّن BODKA المكتبات والإعدادات في مجلد بياناته، ويمكن استخدام المجلد المعتاد بعد التحديث.
3. اترك مجلد `Horizon` بجوار التطبيق لاستخدام مساعد ربط Microsoft المرفق. يمكن أيضًا تنزيل ملف EXE منفردًا. عند طلب الربط، يجلب المشغّل المساعد الرسمي ويتحقق من بصمته. [ملف EXE المنفرد](https://github.com/aboodksa4011/Horizon-Bodka-Ultra-Beta/releases/download/v0.2.0-beta.2/HorizonBodkaUltra.exe)

## اختيار طريقة اللعب

- **مكتبة جديدة** — اختر **إنشاء مكتبة** ثم الإصدار والمحمّل. بعد اكتمال التجهيز، افتح **تعديل** لإدارة المودات والعوالم.
- **تثبيت موجود على الجهاز** — افتح **مشغّل BODKA المزدوج**. اختر إصدارك المكتشف أو حدّد مجلد اللعبة الذي يحتوي على `versions`. اختر **استيراد النسخة** لإنشاء نسخة مُدارة.
- **اللعب كضيف** — يفتح **لعب ماينكرافت كلاسيك كضيف** نسخة Minecraft Classic الرسمية داخل نافذة تستخدم WebView2 وتتطلب اتصالًا بالإنترنت. يحتاج تشغيل Minecraft Java الحديثة إلى حساب يملك اللعبة. اختر **ربط Microsoft عبر هورايزن** واتبع الخطوات في صفحة Microsoft.

## التحقق من ملف التنزيل

شغّل الأمر التالي في PowerShell من مجلد التنزيل، وقارن قيمة `Hash` ببصمة الملف المنشورة مع الإصدار. بناء Beta غير موقّع بشهادة ناشر Windows. [ملف البصمات SHA256SUMS.txt](https://github.com/aboodksa4011/Horizon-Bodka-Ultra-Beta/releases/download/v0.2.0-beta.2/SHA256SUMS.txt)

```powershell
Get-FileHash -Algorithm SHA256 .\Horizon-Bodka-Ultra-Beta-Frosted-Windows-x64.zip
```

[صور التحديث](../README.md) · [تقرير الاختبارات](../BODKA-11-IMPLEMENTATION-REPORT.md)
