# تشغيل BODKA Beta 03 على ويندوز

يدعم الإصدار أجهزة Intel وAMD التي تعمل بويندوز 10 أو 11 بمعمارية x64.

1. افتح GitHub وابحث عن `Horizon-Bodka-Ultra-Beta`. اختر مستودع `aboodksa4011`.
2. انزل إلى وصف المشروع واضغط **تفاصيل الإصدار**. داخل **Assets** نزّل `Horizon-Bodka-Ultra-Java-Beta-Windows-x64.zip`. [صفحة Beta 03](https://github.com/aboodksa4011/Horizon-Bodka-Ultra-Beta/releases/tag/v0.2.0-beta.3)
3. فكّ ضغط الحزمة بالكامل، ثم افتح `HorizonBodkaUltra.exe`. اترك مجلد `Horizon` بجانبه لاستخدام مساعد ربط Microsoft المرفق.

## اختر طريقة اللعب

**الرسمية.** اختر **عندي ماينكرافت الأصلية** وسجّل الدخول بحساب Microsoft الذي يملك Java Edition. اختر الإصدار، وتابع التنزيل، ثم اضغط **العب** في مكتبته.

**عبر TLauncher.** اختر **عندي نسخة محلية أو مكركة** ثم **فحص TLauncher**. إذا كان مثبتًا، اضغط **فتح TLauncher** وأكمل اللعب منه. لا يلزم حساب Microsoft أو نسخة Minecraft الرسمية لفتح هذا المشغّل من BODKA. إذا لم يُكتشف التطبيق، افتح موقعه من صفحة الفحص ثم أعد المحاولة. يمكن اختيار مجلد `versions` من خلال تحديد مجلد اللعبة الأب لاستيراد الملفات المحلية.

**المكتبات المستوردة.** زر **فتح TLauncher** يفتح التطبيق الخارجي. زر **العب** على مكتبة Minecraft Java داخل BODKA يستخدم حسابًا مالكًا للعبة. تتوفر إدارة المكتبات والإعدادات في وضع الضيف.

## بصمة الحزمة

نفّذ الأمر التالي من مجلد التنزيل وقارن قيمة SHA-256 بالملف المنشور مع الإصدار. [تنزيل البصمات](https://github.com/aboodksa4011/Horizon-Bodka-Ultra-Beta/releases/download/v0.2.0-beta.3/SHA256SUMS.txt)

```powershell
Get-FileHash -Algorithm SHA256 .\Horizon-Bodka-Ultra-Java-Beta-Windows-x64.zip
```

[معرض الواجهات والقوائم](BODKA-GALLERY.md) · [الحزمة المباشرة لويندوز](https://github.com/aboodksa4011/Horizon-Bodka-Ultra-Beta/releases/download/v0.2.0-beta.3/Horizon-Bodka-Ultra-Java-Beta-Windows-x64.zip)
