# بنية 𝓗𝓸𝓻𝓲𝔃𝓸𝓷 𝓑𝓸𝓭𝓴𝓪 𝓤𝓵𝓽𝓻𝓪

المصدر: https://github.com/horizon-rust/horizon، على الفرع المحلي `feature/bodka-ultra`، انطلاقًا من `41f822b659789780f58d32c63ddfb6887ea1b8bf`.

المشغّل مساحة عمل Rust مستقلة داخل `launcher/`، بواجهة GPUI Fast. ينفذ التنزيل والتحقق والتجهيز وإدارة النسخ والحسابات عبر الوحدات الموجودة في المشروع.

محرك اللعبة في مساحة العمل الرئيسية. نقطة الدخول في `crates/horizon-client`، والرسم في `horizon-client-render`، والنصوص في `horizon-client-core`، والعوالم في `horizon-world`.

هوية التوزيعة ولغتها الافتراضية في `launcher/src/branding.rs`. ملفات اللغة العربية في `launcher/lang/ar.json` و`crates/horizon-client/assets/horizon/lang/ar_sa.json`.

تتضمن `third_party/gpui-fast-windows` إصلاحًا محليًا لعرض العربية في DirectWrite، موثقًا في `BODKA-PATCH.md` مع الترخيص الأصلي. بقيت معرّفات بروتوكول اللعبة ومعرّفات إصدارات المصدر كما هي.
