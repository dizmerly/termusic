import AppKit

/// Menu row: label above a slider, sized by Auto Layout from the controls so the knob is never clipped.
func sliderRow(_ label: String, _ s: NSSlider) -> NSView {
    let name = NSTextField(labelWithString: label)
    name.font = NSFont.menuFont(ofSize: 0)
    let v = NSView()
    for c in [name, s] { c.translatesAutoresizingMaskIntoConstraints = false; v.addSubview(c) }
    // The knob overhangs the track at both ends, so the slider gets an extra trailing/leading margin beyond the label inset.
    let inset: CGFloat = 16, knobRoom: CGFloat = 4
    NSLayoutConstraint.activate([
        name.leadingAnchor.constraint(equalTo: v.leadingAnchor, constant: inset),
        name.trailingAnchor.constraint(lessThanOrEqualTo: v.trailingAnchor, constant: -inset),
        name.topAnchor.constraint(equalTo: v.topAnchor, constant: 4),
        s.leadingAnchor.constraint(equalTo: v.leadingAnchor, constant: inset + knobRoom),
        s.trailingAnchor.constraint(equalTo: v.trailingAnchor, constant: -(inset + knobRoom)),
        s.topAnchor.constraint(equalTo: name.bottomAnchor, constant: 2),
        s.bottomAnchor.constraint(equalTo: v.bottomAnchor, constant: -4),
        v.widthAnchor.constraint(equalToConstant: 200),
    ])
    v.frame.size = v.fittingSize
    return v
}
