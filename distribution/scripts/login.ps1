function New-RelayLoginForm([string]$Username = 'demo') {
    Add-Type -AssemblyName System.Windows.Forms
    Add-Type -AssemblyName System.Drawing
    [Windows.Forms.Application]::EnableVisualStyles()
    $form = New-Object Windows.Forms.Form
    $form.Text = 'Kokoverse - Sign in'
    $form.ClientSize = New-Object Drawing.Size(420,245)
    $form.StartPosition = 'CenterScreen'
    $form.FormBorderStyle = 'FixedDialog'
    $form.MaximizeBox = $false
    $form.MinimizeBox = $false
    $form.TopMost = $true
    $form.ShowInTaskbar = $true
    $description = New-Object Windows.Forms.Label
    $description.Text = "Realtime connection`r`nEnter the relay login supplied by the host."
    $description.SetBounds(20,15,380,58)
    $form.Controls.Add($description)
    $userLabel = New-Object Windows.Forms.Label
    $userLabel.Text = 'Username'
    $userLabel.SetBounds(20,85,95,24)
    $form.Controls.Add($userLabel)
    $userBox = New-Object Windows.Forms.TextBox
    $userBox.Text = $Username
    $userBox.SetBounds(120,82,275,26)
    $form.Controls.Add($userBox)
    $passwordLabel = New-Object Windows.Forms.Label
    $passwordLabel.Text = 'Password'
    $passwordLabel.SetBounds(20,125,95,24)
    $form.Controls.Add($passwordLabel)
    $passwordBox = New-Object Windows.Forms.TextBox
    $passwordBox.UseSystemPasswordChar = $true
    $passwordBox.SetBounds(120,122,275,26)
    $form.Controls.Add($passwordBox)
    $rememberLabel = New-Object Windows.Forms.Label
    $rememberLabel.Text = 'Saved encrypted for your Windows account on this PC.'
    $rememberLabel.SetBounds(20,160,380,30)
    $form.Controls.Add($rememberLabel)
    $login = New-Object Windows.Forms.Button
    $login.Text = 'Sign in'
    $login.SetBounds(215,200,85,30)
    $login.DialogResult = [Windows.Forms.DialogResult]::OK
    $form.Controls.Add($login)
    $cancel = New-Object Windows.Forms.Button
    $cancel.Text = 'Cancel'
    $cancel.SetBounds(310,200,85,30)
    $cancel.DialogResult = [Windows.Forms.DialogResult]::Cancel
    $form.Controls.Add($cancel)
    $form.AcceptButton = $login
    $form.CancelButton = $cancel
    $form.Tag = @{ Username=$userBox; Password=$passwordBox }
    $form.Add_Shown({ $this.Activate(); $this.Tag.Password.Focus() })
    return $form
}

function Get-RelayLogin([string]$Username = 'demo') {
    $form = New-RelayLoginForm $Username
    try {
        if ($form.ShowDialog() -ne [Windows.Forms.DialogResult]::OK) { return $null }
        if (-not $form.Tag.Username.Text.Trim() -or -not $form.Tag.Password.Text) { throw 'Username and password are required.' }
        $secret = ConvertTo-SecureString $form.Tag.Password.Text -AsPlainText -Force
        return New-Object Management.Automation.PSCredential($form.Tag.Username.Text.Trim(),$secret)
    } finally {
        $form.Tag.Password.Clear()
        $form.Dispose()
    }
}

function Get-PlayerName([string]$InitialName = '') {
    Add-Type -AssemblyName System.Windows.Forms
    Add-Type -AssemblyName System.Drawing
    [Windows.Forms.Application]::EnableVisualStyles()
    $form = New-Object Windows.Forms.Form
    $form.Text = 'Kokoverse'
    $form.ClientSize = New-Object Drawing.Size(380,155)
    $form.StartPosition = 'CenterScreen'
    $form.FormBorderStyle = 'FixedDialog'
    $form.MaximizeBox = $false
    $form.MinimizeBox = $false
    $form.TopMost = $true
    $label = New-Object Windows.Forms.Label
    $label.Text = 'Your name'
    $label.SetBounds(20,18,340,25)
    $form.Controls.Add($label)
    $input = New-Object Windows.Forms.TextBox
    $input.Text = $InitialName
    $input.MaxLength = 40
    $input.SetBounds(20,48,340,28)
    $form.Controls.Add($input)
    $join = New-Object Windows.Forms.Button
    $join.Text = 'Join'
    $join.SetBounds(270,100,90,32)
    $join.DialogResult = [Windows.Forms.DialogResult]::OK
    $join.Enabled = -not [string]::IsNullOrWhiteSpace($InitialName)
    $form.Controls.Add($join)
    $form.AcceptButton = $join
    $input.Add_TextChanged({ $this.Parent.AcceptButton.Enabled = -not [string]::IsNullOrWhiteSpace($this.Text) })
    $form.Tag = $input
    $form.Add_Shown({ $this.Activate(); $this.Tag.Focus(); $this.Tag.SelectAll() })
    try {
        if ($form.ShowDialog() -ne [Windows.Forms.DialogResult]::OK) { return '' }
        return $input.Text.Trim()
    } finally { $form.Dispose() }
}
