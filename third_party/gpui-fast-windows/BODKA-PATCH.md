# إصلاح عرض العربية في ويندوز

المصدر: gpui-fast-windows 0.1.13 كما نُشر في crates.io، من مشروع
https://github.com/longbridge/gpui-fast، بترخيص Apache-2.0 المرفق في LICENSE.
حقوق Zed Industries وLongbridge محفوظة. لم تتضمن الحزمة المنشورة ملف NOTICE.

التعديل في `src/direct_write.rs`: استخدام موضع بداية مجموعة الحروف واتجاهها
`bidiLevel` بدل تجميع كل الحروف من اليسار؛ عكس إزاحات الحروف العربية، والإبقاء
على فهارس UTF-8 الأصلية للنص المختلط. تُرتّب الحروف بصريًا لتلائم رسم GPUI.
كما يسمح محوّل الفهارس بالرجوع عند وصول مجموعات النص بترتيب بصري مختلف.
يحدّد اتجاه الفقرة بواسطة `unicode-bidi`، ثم يضبط `SetReadingDirection`؛
تُحاذى الفقرة العربية إلى الحافة اليسرى داخليًا لأن GPUI يتولى وضع السطر.
مرجع اتجاه الفقرات:
https://learn.microsoft.com/en-us/windows/win32/api/dwrite/nf-dwrite-idwritetextformat-setreadingdirection
أُضيف اعتماد الاختبار الناقص `serde_json` إلى `Cargo.toml` لتشغيل الاختبارات
الأصلية، وأُزيل استيراد `HWND` غير المستخدم من `src/fast/composition.rs`.

مرجع سلوك DirectWrite:
https://learn.microsoft.com/en-us/windows/win32/api/dwrite/ns-dwrite-dwrite_glyph_run

هذا إصلاح للنصوص، ولا يمثل تحسينًا مقاسًا في أداء محرك اللعبة.
